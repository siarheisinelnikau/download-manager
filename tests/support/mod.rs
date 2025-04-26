pub fn fixture_dir() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".test-output");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::TempDir::new_in(root).unwrap()
}
