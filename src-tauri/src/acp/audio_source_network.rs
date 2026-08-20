use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{CONTENT_TYPE, LOCATION};
use tokio::io::AsyncWriteExt;

use crate::acp::audio_transcription::AudioToolFailure;

use super::{audio_content_type, new_temp_path, LoadedAudio};

const MAX_REDIRECTS: usize = 5;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) async fn download(
    source: &str,
    requested_name: Option<&str>,
    max_bytes: u64,
) -> Result<LoadedAudio, AudioToolFailure> {
    let mut url = parse_https_url(source)?;
    for redirect in 0..=MAX_REDIRECTS {
        let client = validated_client(&url).await?;
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|_| AudioToolFailure::download_failed())?;
        if response.status().is_redirection() {
            if redirect == MAX_REDIRECTS {
                return Err(AudioToolFailure::download_failed());
            }
            url = redirect_url(&url, response.headers().get(LOCATION))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(AudioToolFailure::download_failed());
        }
        return store_response(response, &url, requested_name, max_bytes).await;
    }
    Err(AudioToolFailure::download_failed())
}

async fn store_response(
    response: reqwest::Response,
    url: &reqwest::Url,
    requested_name: Option<&str>,
    max_bytes: u64,
) -> Result<LoadedAudio, AudioToolFailure> {
    let mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let name = requested_name
        .map(str::to_string)
        .unwrap_or_else(|| url_file_name(url));
    let suffix = Path::new(&name)
        .extension()
        .and_then(|value| value.to_str());
    let path = new_temp_path(suffix)?;
    write_response(response, &path, max_bytes).await?;
    Ok(LoadedAudio {
        path: path.to_path_buf(),
        file_name: name.clone(),
        content_type: audio_content_type(&name, mime.as_deref()),
        temp_path: Some(path),
    })
}

async fn write_response(
    response: reqwest::Response,
    path: &Path,
    max_bytes: u64,
) -> Result<(), AudioToolFailure> {
    if response
        .content_length()
        .is_some_and(|size| size > max_bytes)
    {
        return Err(AudioToolFailure::too_large());
    }
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|_| AudioToolFailure::download_failed())?;
    let mut total = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AudioToolFailure::download_failed())?;
        total = total.saturating_add(chunk.len() as u64);
        if total > max_bytes {
            return Err(AudioToolFailure::too_large());
        }
        file.write_all(&chunk)
            .await
            .map_err(|_| AudioToolFailure::download_failed())?;
    }
    if total == 0 {
        return Err(AudioToolFailure::download_failed());
    }
    file.flush()
        .await
        .map_err(|_| AudioToolFailure::download_failed())
}

async fn validated_client(url: &reqwest::Url) -> Result<reqwest::Client, AudioToolFailure> {
    let host = url.host_str().ok_or_else(AudioToolFailure::invalid_url)?;
    let port = url
        .port_or_known_default()
        .ok_or_else(AudioToolFailure::invalid_url)?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| AudioToolFailure::download_failed())?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(AudioToolFailure::invalid_url());
    }
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(DOWNLOAD_TIMEOUT)
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|_| AudioToolFailure::download_failed())
}

fn parse_https_url(source: &str) -> Result<reqwest::Url, AudioToolFailure> {
    let url = reqwest::Url::parse(source.trim()).map_err(|_| AudioToolFailure::invalid_url())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AudioToolFailure::invalid_url());
    }
    Ok(url)
}

fn redirect_url(
    current: &reqwest::Url,
    location: Option<&reqwest::header::HeaderValue>,
) -> Result<reqwest::Url, AudioToolFailure> {
    let location = location
        .and_then(|value| value.to_str().ok())
        .ok_or_else(AudioToolFailure::download_failed)?;
    let next = current
        .join(location)
        .map_err(|_| AudioToolFailure::download_failed())?;
    parse_https_url(next.as_str())
}

fn url_file_name(url: &reqwest::Url) -> String {
    let decoded = url
        .path_segments()
        .and_then(|mut values| values.next_back())
        .filter(|value| !value.is_empty())
        .and_then(|value| urlencoding::decode(value).ok())
        .map(|value| value.into_owned());
    let Some(name) = decoded else {
        return "audio.bin".to_string();
    };
    let name = name
        .replace(['/', '\\'], "_")
        .chars()
        .filter(|value| !value.is_control())
        .collect::<String>();
    if name.is_empty() || matches!(name.as_str(), "." | "..") {
        "audio.bin".to_string()
    } else {
        name
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    let shared = octets[0] == 100 && (64..=127).contains(&octets[1]);
    let benchmark = octets[0] == 198 && (18..=19).contains(&octets[1]);
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || octets[0] >= 240
        || shared
        || benchmark)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = ip.segments();
    let unique_local = segments[0] & 0xfe00 == 0xfc00;
    let link_local = segments[0] & 0xffc0 == 0xfe80;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || unique_local
        || link_local
        || documentation)
}
