use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const WEB_DEFAULTS: &str = "$PSDefaultParameterValues['Invoke-WebRequest:UseBasicParsing']=$true;";
const COMMAND_STATUS: &str = "\nif (-not $?) { exit 1 }";
const UTF16_CODE_UNIT_BYTES: usize = 2;

pub(super) fn preferred_shell() -> Option<PathBuf> {
    find_executable("pwsh.exe", "ProgramFiles", "PowerShell/7/pwsh.exe").or_else(|| {
        find_executable(
            "powershell.exe",
            "SystemRoot",
            "System32/WindowsPowerShell/v1.0/powershell.exe",
        )
    })
}

fn find_executable(name: &str, root: &str, relative: &str) -> Option<PathBuf> {
    which::which(name).ok().or_else(|| {
        std::env::var_os(root)
            .map(PathBuf::from)
            .map(|path| path.join(relative))
            .filter(|path| path.is_file())
    })
}

pub(super) fn prompt() -> String {
    let shell = preferred_shell()
        .map(|path| format!("`{}`", path.display()))
        .unwrap_or_else(|| "unavailable; report a missing PowerShell executable".to_string());
    format!(
        "## Windows command execution\n\n\
         Preferred PowerShell executable detected on this host: {shell}. Use this path when the tool accepts a shell selection.\n\
         For automatic PowerShell commands, use -NoProfile -NonInteractive. Do not assume PowerShell 7 is installed at a fixed path.\n\
         In Windows PowerShell 5.1, every Invoke-WebRequest call (including iwr, curl and wget aliases) must use -UseBasicParsing. For a script with multiple requests, set {WEB_DEFAULTS} in that same PowerShell process before running it. Defaults do not propagate into nested powershell.exe processes.\n\
         For native curl, use curl.exe explicitly. A -NonInteractive error does not mean the download succeeded; fix the request parameters before retrying. Do not disable security checks or automatically answer confirmation prompts."
    )
}

pub(super) fn command(program: &str, args: &[String]) -> Result<tokio::process::Command, String> {
    let mut command = crate::process::tokio_command(program);
    if !is_powershell(program) {
        command.args(args);
        return Ok(command);
    }
    let boundary = args.iter().position(|arg| is_script_parameter(arg));
    let options = &args[..boundary.unwrap_or(args.len())];
    if options.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        (lower.len() >= "-noe".len() && "-noexit".starts_with(&lower)) || lower == "-interactive"
    }) {
        return Err("ACP PowerShell commands must exit; use the interactive terminal for -NoExit or -Interactive".to_string());
    }
    command.args(["-NoProfile", "-NonInteractive"]);
    let Some(index) = boundary else {
        command.args(args);
        return Ok(command);
    };
    let parameter = args[index].to_ascii_lowercase();
    if matches!(parameter.as_str(), "-file" | "-f") {
        // 保留 -File 的参数绑定、PSScriptRoot 与原生退出码；文件中的请求需显式使用安全解析。
        command.args(args);
        return Ok(command);
    }
    let script = script_body(&parameter, &args[index + 1..])?;
    let script = bootstrap(&script);
    command.args(options);
    if matches!(parameter.as_str(), "-command" | "-c") {
        command.arg("-Command").arg(script);
    } else {
        let bytes = script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        command.arg("-EncodedCommand").arg(STANDARD.encode(bytes));
    }
    tracing::debug!("[ACP] PowerShell command configured for noninteractive web requests");
    Ok(command)
}

fn is_powershell(program: &str) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
    )
}

fn is_script_parameter(argument: &str) -> bool {
    matches!(
        argument.to_ascii_lowercase().as_str(),
        "-command" | "-c" | "-encodedcommand" | "-enc" | "-ec" | "-e" | "-file" | "-f"
    )
}

fn script_body(parameter: &str, arguments: &[String]) -> Result<String, String> {
    if arguments.is_empty() || arguments.first().is_some_and(|value| value == "-") {
        return Err(
            "ACP PowerShell requires a script argument; stdin is not interactive".to_string(),
        );
    }
    if matches!(parameter, "-command" | "-c") {
        return Ok(arguments.join(" "));
    }
    if arguments.len() != 1 {
        return Err("ACP PowerShell -EncodedCommand requires exactly one argument".to_string());
    }
    let bytes = STANDARD
        .decode(&arguments[0])
        .map_err(|_| "PowerShell -EncodedCommand must contain valid Base64".to_string())?;
    if bytes.len() % UTF16_CODE_UNIT_BYTES != 0 {
        return Err("PowerShell -EncodedCommand must contain UTF-16LE".to_string());
    }
    let units = bytes
        .chunks_exact(UTF16_CODE_UNIT_BYTES)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units)
        .map_err(|_| "PowerShell -EncodedCommand must contain UTF-16LE".to_string())
}

fn bootstrap(script: &str) -> String {
    // 独立解析原脚本，保留开头的 param/using 声明，且不展开脚本中的变量或引号。
    // 在原脚本作用域读取 $?，避免 PS 5.1 的调用运算符把非终止错误变成成功。
    let script = format!("{script}{COMMAND_STATUS}");
    format!(
        "{WEB_DEFAULTS} & ([scriptblock]::Create('{}'))",
        script.replace('\'', "''")
    )
}
