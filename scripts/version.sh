read_version() {
  sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1
}

write_version() {
  sed -i '' "s/^version = \".*\"/version = \"$1\"/" Cargo.toml
  cargo update --workspace --quiet
}
