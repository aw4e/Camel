// Gives camel.exe its icon (Explorer, taskbar, Alt+Tab). Only Windows builds embed it.
fn main() {
    println!("cargo:rerun-if-changed=assets/brand/camel.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        winresource::WindowsResource::new().set_icon("assets/brand/camel.ico").compile().expect("embedding the icon failed");
    }
}
