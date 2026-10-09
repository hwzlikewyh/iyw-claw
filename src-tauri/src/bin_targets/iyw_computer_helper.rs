//! 独立服务端执行器的兼容入口；桌面端通过主程序内部参数执行。

use iyw_claw_lib::computer::helper::run;
use iyw_claw_lib::computer::permission_request::request_permission;
use iyw_claw_lib::computer::protocol::REQUEST_PERMISSION_ARG;

fn main() {
    let mut args = std::env::args().skip(1);
    let argument = args.next();
    if argument.as_deref() == Some("--version") {
        println!("iyw-computer-helper {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if argument.as_deref() == Some("--identity") {
        println!(
            "{}",
            serde_json::json!({"version": env!("CARGO_PKG_VERSION"),
            "source": iyw_claw_lib::computer::protocol::SOURCE_FINGERPRINT,
            "target": env!("IYW_CLAW_TARGET_TRIPLE")})
        );
        return;
    }
    if argument.as_deref() == Some(REQUEST_PERMISSION_ARG) {
        std::process::exit(request_permission(args.next().as_deref()));
    }
    std::process::exit(run());
}
