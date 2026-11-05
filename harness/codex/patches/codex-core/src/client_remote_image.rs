use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, bail};
use codex_http_client::{ClientRouteClass, HttpClient};
use codex_login::default_client::{ClientRedirectPolicy, create_client_for_route};
use futures::StreamExt;
use http::header::{ACCEPT, LOCATION};
use url::Url;

use crate::client::ModelClient;

const IMAGE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

impl ModelClient {
    pub(crate) async fn download_remote_image(&self, source: &str) -> anyhow::Result<Vec<u8>> {
        let mut url = Url::parse(source).context("invalid remote image URL")?;
        for redirects in 0..=MAX_REDIRECTS {
            validate_public_https_url(&url).await?;
            let client = remote_image_client(self, &url)?;
            let response = client
                .get(url.clone())
                .header(
                    ACCEPT,
                    "image/png,image/jpeg,image/gif,image/webp,image/bmp",
                )
                .timeout(IMAGE_DOWNLOAD_TIMEOUT)
                .send()
                .await
                .context("remote image request failed")?;
            if response.status().is_redirection() {
                if redirects == MAX_REDIRECTS {
                    bail!("remote image has too many redirects");
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .context("remote image redirect has no valid location")?;
                url = url
                    .join(location)
                    .context("remote image redirect URL is invalid")?;
                continue;
            }
            if !response.status().is_success() {
                bail!("remote image returned HTTP {}", response.status());
            }
            return read_remote_image(response).await;
        }
        bail!("remote image redirect failed")
    }
}

fn remote_image_client(client: &ModelClient, url: &Url) -> anyhow::Result<HttpClient> {
    create_client_for_route(
        &client.http_client_factory,
        url.as_str(),
        ClientRouteClass::Api,
        ClientRedirectPolicy::Reject,
    )
    .map(HttpClient::without_request_logging)
    .context("cannot create remote image client")
}

async fn validate_public_https_url(url: &Url) -> anyhow::Result<()> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("remote image URL must use HTTPS without credentials or a fragment");
    }
    let host = url.host_str().unwrap_or_default().to_owned();
    let port = url
        .port_or_known_default()
        .context("remote image URL has no usable port")?;
    let addresses = tokio::task::spawn_blocking(move || {
        (host.as_str(), port)
            .to_socket_addrs()
            .map(|values| values.map(|value| value.ip()).collect::<Vec<_>>())
    })
    .await
    .context("remote image host lookup task failed")?
    .context("remote image host lookup failed")?;
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(*address)) {
        bail!("remote image URL must resolve only to public addresses");
    }
    Ok(())
}

async fn read_remote_image(response: codex_http_client::HttpResponse) -> anyhow::Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
    {
        bail!("remote image exceeds the 20 MiB limit");
    }
    let capacity = response
        .content_length()
        .unwrap_or_default()
        .min(MAX_IMAGE_BYTES as u64) as usize;
    let mut bytes = Vec::with_capacity(capacity);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("remote image response body failed")?;
        if bytes.len().saturating_add(chunk.len()) > MAX_IMAGE_BYTES {
            bail!("remote image exceeds the 20 MiB limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        bail!("remote image is empty");
    }
    Ok(bytes)
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
    let global_unicast = segments[0] & 0xe000 == 0x2000;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    global_unicast && !documentation && !ip.is_multicast()
}
