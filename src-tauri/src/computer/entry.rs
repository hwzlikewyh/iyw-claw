//! 主程序内置电脑执行器；必须在 GUI、日志和数据库初始化前分流。

pub const HELPER_ARG: &str = "--internal-computer-helper";

pub fn dispatch_early() -> bool {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(HELPER_ARG)) {
        return false;
    }
    let operation = args.next();
    let code = match operation.as_ref().and_then(|argument| argument.to_str()) {
        None if operation.is_none() => super::helper::run(),
        Some("--identity") => {
            println!(
                "{}",
                serde_json::json!({
                    "mode": "same-executable",
                    "version": env!("CARGO_PKG_VERSION"),
                    "source": super::protocol::SOURCE_FINGERPRINT,
                    "target": env!("IYW_CLAW_TARGET_TRIPLE"),
                })
            );
            return true;
        }
        Some(super::protocol::REQUEST_PERMISSION_ARG) => {
            let permission = args.next();
            super::permission_request::request_permission(
                permission.as_ref().and_then(|name| name.to_str()),
            )
        }
        _ => {
            eprintln!("无效的内置电脑操作参数");
            super::helper::EXIT_FAILED
        }
    };
    std::process::exit(code);
}

pub(super) fn launch_args() -> &'static [&'static str] {
    if cfg!(feature = "tauri-runtime") {
        &[HELPER_ARG]
    } else {
        &[]
    }
}
