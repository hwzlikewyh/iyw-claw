use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, LOCATION};

use crate::app_error::AppCommandError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 5;

pub struct DownloadedImage {
    pub bytes: Vec<u8>,
    pub final_url: reqwest::Url,
    pub redirects: usize,
}

pub async fn download(source: &str, max_bytes: usize) -> Result<DownloadedImage, AppCommandError> {
    let mut url = parse_url(source)?;
    for redirects in 0..=MAX_REDIRECTS {
        let client = validated_client(&url).await?;
        let response = client
            .get(url.clone())
            .header(
                ACCEPT,
                "image/png,image/jpeg,image/gif,image/webp,image/bmp",
            )
            .send()
            .await
            .map_err(request_error)?;
        if response.status().is_redirection() {
            if redirects == MAX_REDIRECTS {
                return Err(AppCommandError::network(
                    "Remote image has too many redirects",
                ));
            }
            url = redirect_url(&url, response.headers().get(LOCATION))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(AppCommandError::network(format!(
                "Remote image returned HTTP {}",
                response.status()
            )));
        }
        let bytes = read_limited(response, max_bytes).await?;
        return Ok(DownloadedImage {
            bytes,
            final_url: url,
            redirects,
        });
    }
    Err(AppCommandError::network("Remote image redirect failed"))
}

fn parse_url(source: &str) -> Result<reqwest::Url, AppCommandError> {
    let url = reqwest::Url::parse(source).map_err(|error| {
        AppCommandError::invalid_input("Invalid remote image URL").with_detail(error.to_string())
    })?;
    validate_url_shape(&url)?;
    Ok(url)
}

fn validate_url_shape(url: &reqwest::Url) -> Result<(), AppCommandError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppCommandError::invalid_input(
            "Remote images must use HTTP or HTTPS",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppCommandError::invalid_input(
            "Remote image URLs must not contain credentials",
        ));
    }
    if url.host_str().is_none() {
        return Err(AppCommandError::invalid_input(
            "Remote image URL is missing a host",
        ));
    }
    Ok(())
}

async fn validated_client(url: &reqwest::Url) -> Result<reqwest::Client, AppCommandError> {
    validate_url_shape(url)?;
    let host = url.host_str().unwrap();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| AppCommandError::invalid_input("Remote image URL has no usable port"))?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            AppCommandError::network("Remote image host lookup failed")
                .with_detail(error.to_string())
        })?
        .collect();
    if addresses.is_empty() {
        return Err(AppCommandError::network(
            "Remote image host resolved to no addresses",
        ));
    }
    if addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(AppCommandError::permission_denied(
            "Remote image URL resolves to a non-public address",
        ));
    }
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("iyw-claw/", env!("CARGO_PKG_VERSION")))
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|error| {
            AppCommandError::network("Cannot create remote image HTTP client")
                .with_detail(error.without_url().to_string())
        })
}

fn redirect_url(
    current: &reqwest::Url,
    location: Option<&reqwest::header::HeaderValue>,
) -> Result<reqwest::Url, AppCommandError> {
    let location = location
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            AppCommandError::network("Remote image redirect is missing a valid location")
        })?;
    let next = current.join(location).map_err(|error| {
        AppCommandError::network("Remote image redirect URL is invalid")
            .with_detail(error.to_string())
    })?;
    validate_url_shape(&next)?;
    Ok(next)
}

async fn read_limited(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, AppCommandError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(AppCommandError::invalid_input(
            "Remote image exceeds the 20 MiB limit",
        ));
    }
    let capacity = response.content_length().unwrap_or(0).min(max_bytes as u64) as usize;
    let mut bytes = Vec::with_capacity(capacity);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(request_error)?;
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(AppCommandError::invalid_input(
                "Remote image exceeds the 20 MiB limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(AppCommandError::invalid_input("Remote image is empty"));
    }
    Ok(bytes)
}

fn request_error(error: reqwest::Error) -> AppCommandError {
    AppCommandError::network("Remote image request failed")
        .with_detail(error.without_url().to_string())
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
    let protocol = octets[0] == 192 && octets[1] == 0 && octets[2] == 0;
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
        || benchmark
        || protocol)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = ip.segments();
    let unique_local = segments[0] & 0xfe00 == 0xfc00;
    let link_local = segments[0] & 0xffc0 == 0xfe80;
    let site_local = segments[0] & 0xffc0 == 0xfec0;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || unique_local
        || link_local
        || site_local
        || documentation)
}
