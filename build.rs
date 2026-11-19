// Embeds the app icon into rmcl.exe (Explorer, taskbar, installer shortcuts).
// If the Windows resource compiler is missing the build still succeeds, just without the exe icon.
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let _ = embed_resource::compile("app.rc", embed_resource::NONE);
    }
}
