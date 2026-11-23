//! Embeds the app icon as resource ID 1, which gpui loads for the window
//! (title bar, taskbar, Alt+Tab) and which the browser registration points at.
//! Regenerate the icon with `cargo run --example gen_icon`.

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id("assets/app.ico", "1");
        res.set("FileDescription", "Crossroads");
        res.set("ProductName", "Crossroads");
        res.compile().expect("failed to embed Windows resources");
    }
}
