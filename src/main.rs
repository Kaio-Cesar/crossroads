// Always a GUI app, so opening a link never flashes a console window.
#![windows_subsystem = "windows"]

mod ipc;
mod model;
mod router;
mod ui;

use router::Mode;

fn main() {
    attach_parent_console();

    match Mode::from_args(std::env::args()) {
        Mode::Route(url) => {
            // Fast path: no UI unless the rules ask for the picker.
            if let Some(request) = router::route(url, false) {
                ui::picker::run(request);
            }
        }
        Mode::Pick(url) => {
            if let Some(request) = router::route(url, true) {
                ui::picker::run(request);
            }
        }
        Mode::Diagnose => router::diagnose(),
        Mode::Settings => {
            // If the settings window is already open, bring it forward instead.
            if ipc::forward(&ipc::Request::Show) {
                return;
            }
            ui::settings::run(ipc::serve());
        }
    }
}

/// When started from a terminal (e.g. `cargo run`), reuse that terminal for
/// log output. Started from Explorer there is no parent console: no-op.
fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
