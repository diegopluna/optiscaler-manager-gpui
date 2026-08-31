fn main() {
    // Embed the app icon into the Windows executable so Explorer, the
    // taskbar and the Start Menu show it. Other platforms need nothing.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/icon/optiscaler-manager.ico");
        if let Err(err) = res.compile() {
            println!("cargo:warning=embedding the icon failed: {err}");
        }
    }
}
