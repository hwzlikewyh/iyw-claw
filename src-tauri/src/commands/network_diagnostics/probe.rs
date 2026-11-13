use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::{header, Client, RequestBuilder, Response};

use super::{NetworkProbeKind, NetworkProbeResult};
use crate::app_error::AppCommandError;

const DOMESTIC_LATENCY_TARGET: &str = "connect.rom.miui.com";
const DOMESTIC_LATENCY_URL: &str = "https://connect.rom.miui.com/generate_204";
// SpeedTest.cn 的公开南京节点，测速接口不经过应用服务端的代理层。
const TRANSFER_TARGET: &str = "223.160.155.255:51090";
const DOWNLOAD_URL: &str = "http://223.160.155.255:51090/download?size=1";
const UPLOAD_URL: &str = "http://223.160.155.255:51090/upload";
const DOWNLOAD_BYTES: usize = 1_000_000;
const UPLOAD_BYTES: usize = 256_000;
const RESPONSE_LIMIT_BYTES: usize = DOWNLOAD_BYTES + 16_384;
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const BITS_PER_MEGABIT: f64 = 1_000_000.0;

static PROBE_RUNNING: AtomicBool = AtomicBool::new(false);

struct ProbeGuard;

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        PROBE_RUNNING.store(false, Ordering::Release);
    }
}

pub(super) async fn measure(kind: NetworkProbeKind) -> Result<NetworkProbeResult, AppCommandError> {
    if PROBE_RUNNING.swap(true, Ordering::AcqRel) {
        return Err(AppCommandError::new(
            crate::app_error::AppErrorCode::Conflict,
            "A network measurement is already running",
        ));
    }
    let _guard = ProbeGuard;
    let target_host = target_host(kind);
    tracing::info!(?kind, target_host, "network measurement started");
    let result = measure_request(kind).await;
    match &result {
        Ok(value) => tracing::info!(
            ?kind, target_host, elapsed_ms = value.elapsed_ms,
            bytes = value.bytes, mbps = ?value.mbps, "network measurement completed"
        ),
        Err(error) => tracing::warn!(
            ?kind, target_host, error = %error, detail = ?error.detail,
            "network measurement failed"
        ),
    }
    result
}

async fn measure_request(kind: NetworkProbeKind) -> Result<NetworkProbeResult, AppCommandError> {
    // 与应用其余 reqwest 请求一致，遵循当前系统代理设置。
    let client = Client::builder()
        .timeout(PROBE_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(request_error)?;
    let request = build_request(&client, kind);
    let started = Instant::now();
    let response = request
        .header(header::CACHE_CONTROL, "no-cache, no-store")
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(request_error)?;
    if !response.status().is_success() {
        return Err(AppCommandError::network(format!(
            "Measurement endpoint returned HTTP {}",
            response.status()
        )));
    }
    let received = read_response(response, kind).await?;
    let elapsed = started.elapsed();
    let bytes = measured_bytes(kind, received)?;
    Ok(NetworkProbeResult {
        kind,
        target: target_host(kind),
        elapsed_ms: elapsed.as_secs_f64() * 1_000.0,
        bytes,
        mbps: (!matches!(kind, NetworkProbeKind::Latency))
            .then(|| bytes as f64 * 8.0 / elapsed.as_secs_f64() / BITS_PER_MEGABIT),
    })
}

fn build_request(client: &Client, kind: NetworkProbeKind) -> RequestBuilder {
    match kind {
        NetworkProbeKind::Latency => client.get(DOMESTIC_LATENCY_URL),
        NetworkProbeKind::Upload => client
            .post(UPLOAD_URL)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(vec![0_u8; UPLOAD_BYTES]),
        NetworkProbeKind::Download => client.get(DOWNLOAD_URL),
    }
}

fn target_host(kind: NetworkProbeKind) -> &'static str {
    match kind {
        NetworkProbeKind::Latency => DOMESTIC_LATENCY_TARGET,
        NetworkProbeKind::Download | NetworkProbeKind::Upload => TRANSFER_TARGET,
    }
}

async fn read_response(
    mut response: Response,
    kind: NetworkProbeKind,
) -> Result<usize, AppCommandError> {
    let mut received = 0;
    while let Some(chunk) = response.chunk().await.map_err(request_error)? {
        received += chunk.len();
        if matches!(kind, NetworkProbeKind::Download) && received >= DOWNLOAD_BYTES {
            return Ok(DOWNLOAD_BYTES);
        }
        if received > RESPONSE_LIMIT_BYTES {
            return Err(AppCommandError::network(
                "Measurement response exceeds the size limit",
            ));
        }
    }
    Ok(received)
}

fn measured_bytes(kind: NetworkProbeKind, received: usize) -> Result<usize, AppCommandError> {
    match kind {
        NetworkProbeKind::Download if received != DOWNLOAD_BYTES => Err(AppCommandError::network(
            "Measurement download is incomplete",
        )
        .with_detail(format!("expected={DOWNLOAD_BYTES}, received={received}"))),
        NetworkProbeKind::Upload => Ok(UPLOAD_BYTES),
        _ => Ok(received),
    }
}

fn request_error(error: reqwest::Error) -> AppCommandError {
    let message = if error.is_timeout() {
        "Network measurement timed out (8 seconds)"
    } else if error.is_connect() {
        "Could not connect to the measurement endpoint"
    } else {
        "Network measurement request failed"
    };
    AppCommandError::network(message).with_detail(error.to_string())
}
