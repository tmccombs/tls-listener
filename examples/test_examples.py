#!/usr/bin/env python3
"""
This script is intended to test the examples in the `examples` folder.
"""

import signal
import socket
import ssl
import subprocess
import time
import unittest
from contextlib import contextmanager
from http.client import HTTPConnection
from os import path
from urllib.request import HTTPSHandler, build_opener

HOST = "localhost"
PORT = 3000
DOMAIN = "test.local"
EXAMPLE_DIR = path.dirname(__file__)
CA_FILE = path.join(EXAMPLE_DIR, "tls_config", "localcert.pem")

context = ssl.create_default_context(cafile=CA_FILE)
context.options = ssl.PROTOCOL_TLS_CLIENT


class ExampleHttpsConnection(HTTPConnection):
    def connect(self):
        super().connect()
        self.sock = context.wrap_socket(self.sock, server_hostname=DOMAIN)


class ExampleHttpsHandler(HTTPSHandler):
    def __init__(self, debuglevel=0):
        super().__init__(debuglevel, context=context)

    def https_open(self, req):
        return self.do_open(ExampleHttpsConnection, req)


opener = build_opener(ExampleHttpsHandler)


@contextmanager
def tls_conn():
    with socket.create_connection((HOST, PORT)) as sock:
        with context.wrap_socket(sock, server_hostname=DOMAIN) as tls:
            yield tls
            tls.shutdown(socket.SHUT_RDWR)


def build_examples():
    proc = subprocess.run(
        [
            "cargo",
            "build",
            "--examples",
            "--features",
            "rustls-aws-lc,rt,tokio/rt-multi-thread",
        ]
    )
    proc.check_returncode()


@contextmanager
def run_example(name):
    proc = subprocess.Popen(
        path.join(EXAMPLE_DIR, "..", "target", "debug", "examples", name),
    )
    try:
        time.sleep(0.1)  # wait for process to start up
        yield proc
    finally:
        proc.terminate()
        proc.wait()  # avoid warning about process still running


class TestExamples(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        build_examples()

    def run_echo_test(self, conn):
        r = conn.makefile("r")
        w = conn.makefile("w")

        w.write("hello\n")
        w.write("world\n")
        w.write("it's great\n")
        w.flush()
        for line in ["hello\n", "world\n", "it's great\n"]:
            self.assertEqual(r.readline(), line)
        w.write("goodbye\n")
        w.flush()
        self.assertEqual(r.readline(), "goodbye\n")

    def echo_test(self):
        with tls_conn() as tls:
            self.run_echo_test(tls)

    def http_test(self):
        with opener.open(f"https://{HOST}:{PORT}") as resp:
            self.assertEqual(resp.status, 200)
            self.assertEqual(resp.read(), b"Hello, World!")

    def bad_connection_test(self, proc):
        with socket.create_connection((HOST, PORT)) as s:
            s.send(b"bad data")
        self.assertIsNone(proc.poll(), "Bad data shouldn't kill process")

    def test_echo(self):
        with run_example("echo") as r:
            f"pid={r.pid}"
            self.echo_test()
            with tls_conn() as t:
                r.send_signal(signal.SIGINT)
                time.sleep(0.1)
                self.assertIsNone(
                    r.poll(), "Process should wait for connections to end on ctrl-c"
                )
                self.run_echo_test(t)
            r.wait(0.5)  # process should finish shortly after connection finishes

    def test_echo_threads(self):
        with run_example("echo-threads"):
            self.echo_test()

    def test_http_stream(self):
        with run_example("http-stream") as r:
            self.http_test()
            self.bad_connection_test(r)

    def test_http_plain(self):
        with run_example("http"):
            self.http_test()
