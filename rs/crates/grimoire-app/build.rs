//! Embeds the app icon and version info (which code signing checks) in the Windows .exe (Explorer, the taskbar and Alt-Tab show it).
//! Only runs when building on Windows for Windows; other builds don't need it.

fn main() {
    println!("cargo:rerun-if-changed=assets/grimoire.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/grimoire.ico");
        resource.set("ProductName", "Grimoire");
        resource.set("FileDescription", "Grimoire");
        resource.set("CompanyName", "neonspellcraft");
        resource.set("LegalCopyright", "Grimoire contributors, GNU GPL v2 or later");
        resource.set("OriginalFilename", "grimoire.exe");
        if let Err(e) = resource.compile() {
            println!("cargo:warning=couldn't embed the Windows icon: {e}");
        }
    }
}
