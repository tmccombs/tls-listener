$0 == "[package]" { inpkg = 1; }
inpkg && /^version +=/ {
  sub(/"[^"]*"/, "\"" ENVIRON["VERSION"] "\"")
  inpkg = 0
}
{ print }
