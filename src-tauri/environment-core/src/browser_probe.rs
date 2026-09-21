use std::io::Cursor;
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

use crate::probe_process;

const START_TIMEOUT: Duration = Duration::from_secs(30);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const PROBE_URL: &str = "data:text/html,<html><body><h1>iyw-environment-ready</h1></body></html>";
type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

pub fn verify(executable: &Path, scratch: &Path) -> Result<()> {
    let mut command = Command::new(executable);
    command
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--remote-debugging-port=0",
            "--window-size=640,480",
        ])
        .arg(format!(
            "--user-data-dir={}",
            scratch.join("profile").display()
        ))
        .arg(PROBE_URL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    probe_process::configure(&mut command);
    #[cfg(windows)]
    command.arg("--do-not-de-elevate");
    let mut child = command.spawn().context("Chromix 启动失败")?;
    let job = match crate::probe_job::ProbeJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            probe_process::terminate(&mut child);
            return Err(error);
        }
    };
    let result = inspect(&mut child, scratch);
    probe_process::terminate(&mut child);
    drop(job);
    result
}

fn inspect(child: &mut Child, scratch: &Path) -> Result<()> {
    let port = wait_port(child, &scratch.join("profile/DevToolsActivePort"))?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(IO_TIMEOUT)
        .build()?;
    let targets: Vec<Value> = client
        .get(format!("http://127.0.0.1:{port}/json/list"))
        .send()?
        .error_for_status()?
        .json()?;
    let url = targets
        .iter()
        .find(|item| item["type"] == "page")
        .and_then(|item| item["webSocketDebuggerUrl"].as_str())
        .context("Chromix 没有可控制的页面")?;
    let parsed = reqwest::Url::parse(url)?;
    if parsed.host_str() != Some("127.0.0.1") || parsed.port() != Some(port) {
        bail!("invalid probe CDP URL")
    }
    let (mut socket, _) = tungstenite::connect(url)?;
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
    }
    cdp(&mut socket, 1, ("Page.navigate", json!({"url":PROBE_URL})))?;
    wait_document(&mut socket)?;
    let screenshot = cdp(
        &mut socket,
        3,
        ("Page.captureScreenshot", json!({"format":"png"})),
    )?;
    verify_pixels(
        screenshot["data"]
            .as_str()
            .context("Chromix 没有返回截图")?,
    )?;
    let _ = socket.close(None);
    Ok(())
}

fn wait_document(socket: &mut Socket) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < IO_TIMEOUT {
        let page = cdp(
            socket,
            2,
            (
                "Runtime.evaluate",
                json!({"expression":"document.body?.innerText","returnByValue":true}),
            ),
        )?;
        if page["result"]["value"]
            .as_str()
            .is_some_and(|text| text.contains("iyw-environment-ready"))
        {
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    bail!("Chromix 未能在时限内渲染本地页面")
}

fn wait_port(child: &mut Child, file: &Path) -> Result<u16> {
    let started = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(file) {
            if let Some(port) = text
                .lines()
                .next()
                .and_then(|value| value.parse::<u16>().ok())
            {
                if port > 0 {
                    return Ok(port);
                }
            }
        }
        if let Some(status) = child.try_wait()? {
            bail!("Chromix 提前退出（{status}），请检查系统运行库或安全软件")
        }
        if started.elapsed() >= START_TIMEOUT {
            bail!("Chromix 本地调试连接超时")
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn cdp(socket: &mut Socket, id: u64, request: (&str, Value)) -> Result<Value> {
    socket.send(Message::Text(
        json!({"id":id,"method":request.0,"params":request.1})
            .to_string()
            .into(),
    ))?;
    let started = Instant::now();
    while started.elapsed() < IO_TIMEOUT {
        let message = socket.read()?;
        let Message::Text(text) = message else {
            continue;
        };
        let response: Value = serde_json::from_str(&text)?;
        if response["id"].as_u64() != Some(id) {
            continue;
        }
        if response.get("error").is_some() {
            bail!("Chromix CDP 检查失败：{}", response["error"]);
        }
        return Ok(response["result"].clone());
    }
    bail!("Chromix CDP 检查超时")
}

fn verify_pixels(encoded: &str) -> Result<()> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    if reader.info().width > 2048 || reader.info().height > 2048 {
        bail!("unexpected probe screenshot dimensions")
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    let channels = info.color_type.samples();
    let data = &pixels[..info.buffer_size()];
    if data.is_empty()
        || !data
            .chunks_exact(channels)
            .any(|pixel| pixel != &data[..channels])
    {
        bail!("Chromix 返回空白截图")
    }
    Ok(())
}
