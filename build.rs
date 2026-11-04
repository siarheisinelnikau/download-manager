#[path = "src/icons.rs"]
mod icons;

fn main() {
    println!("cargo:rerun-if-changed=src/icons.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("DownloadManager.ico");
    std::fs::write(&path, icons::ico()).expect("Cannot write the generated application icon");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let architecture = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "amd64",
        Ok("aarch64") => "arm64",
        Ok("x86") => "i386",
        _ => panic!("Unsupported Windows architecture"),
    };
    winresource::WindowsResource::new()
        .set_icon(path.to_str().unwrap())
        .set("ProductName", "DownloadManager")
        .set("FileDescription", "DownloadManager")
        .set("FileVersion", &version)
        .set("ProductVersion", &version)
        .set(
            "OriginalFilename",
            &format!("DownloadManager-v{version}-windows-{architecture}.exe"),
        )
        .compile()
        .expect("Cannot embed Windows icon and version resources");
}
