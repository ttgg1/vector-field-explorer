// Embed the Windows application icon into the .exe. Gated to the Windows
// target so the Linux/macOS/wasm builds are untouched (ticket 72).
fn main() {
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR is set by Cargo");
        let icon = format!("{manifest_dir}/packaging/windows/vector-field-explorer.ico");
        let mut res = winres::WindowsResource::new();
        res.set_icon(&icon);
        res.compile()
            .expect("failed to compile Windows resource (rc.exe missing?)");
    }
}
