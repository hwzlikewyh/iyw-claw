use reqwest::{Response, Url};
use sea_orm::DatabaseConnection;
use serde_json::Value;

use super::{market, market_types::Document};
use crate::app_error::AppCommandError;

const DOWNLOAD_TIMEOUT_SECONDS: u64 = 30 * 60;

pub(super) async fn download(
    conn: &DatabaseConnection,
    input: (&Document, &str, &Value),
) -> Result<Response, AppCommandError> {
    let (document, digest, metadata) = input;
    let ticket = metadata["ticket"]
        .as_str()
        .ok_or_else(|| AppCommandError::network("制品下载票据缺失"))?;
    let issued = market::request(
        conn,
        "artifact-ticket",
        serde_json::json!({
            "connector": document.server_id, "id": digest, "ticket": ticket,
        }),
    )
    .await?;
    let url = validate_ticket(&issued, digest, metadata)?;
    let response = market::client()?
        .get(url)
        .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECONDS))
        .send()
        .await
        .map_err(|_| AppCommandError::network("连接器文件下载失败"))?;
    if !response.status().is_success() {
        return Err(AppCommandError::network(
            "连接器制品下载失败，请重试以刷新下载授权",
        ));
    }
    Ok(response)
}

fn validate_ticket(issued: &Value, digest: &str, metadata: &Value) -> Result<Url, AppCommandError> {
    let size = issued["size"].as_u64().filter(|value| *value > 0);
    if size.is_none()
        || size != metadata["size"].as_u64()
        || issued["sha256"].as_str() != Some(digest)
    {
        return Err(AppCommandError::configuration_invalid(
            "连接器下载授权与安装计划不一致",
        ));
    }
    let raw = issued["url"]
        .as_str()
        .ok_or_else(|| AppCommandError::configuration_invalid("连接器下载地址缺失"))?;
    let url = Url::parse(raw)
        .map_err(|_| AppCommandError::configuration_invalid("连接器下载地址无效"))?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if (url.scheme() != "https" && !(cfg!(debug_assertions) && local && url.scheme() == "http"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AppCommandError::configuration_invalid("连接器下载地址无效"));
    }
    Ok(url)
}
