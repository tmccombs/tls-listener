macro_rules! assert_err {
    ($ex:expr, $m:pat) => {
        match $ex {
            Err($m) => (),
            Err(e) => panic!("Unexpected error: {:?}", e),
            Ok(_) => panic!("Expected error, but got Ok"),
        }
    };
}

macro_rules! assert_ascii_eq {
    ($one:expr, $two:expr_2021) => {
        assert_eq!(
            ::std::str::from_utf8(&*$one).unwrap(),
            ::std::str::from_utf8(&*$two).unwrap()
        )
    };
}

pub(crate) use assert_ascii_eq;
pub(crate) use assert_err;
