$0 == "[package]" { inpkg = 1; }
inpkg && /^version +=/ {
  # Strip off any leading "v" in version
  version = ENVIRON["VERSION"]
  sub(/^v/, "", version)
  sub(/"[^"]*"/, "\"" version "\"")
  inpkg = 0
}
{ print }
