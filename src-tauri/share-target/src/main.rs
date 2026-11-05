#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod receiver;

fn main() {
    #[cfg(windows)]
    if let Err(error) = receiver::run() {
        if !std::env::args().any(|arg| arg == "--registration-check") {
            receiver::log(&format!("share reception failed: {error}"));
        }
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("The native share target is only available on Windows.");
        std::process::exit(1);
    }
}
