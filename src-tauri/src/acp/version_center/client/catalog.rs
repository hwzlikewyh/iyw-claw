use reqwest::Method;
use sea_orm::DatabaseConnection;

use super::{catalog_query, decode_response, network_error, AgentPlatformClient};
use crate::acp::version_center::{capability, types::CatalogSnapshot};
use crate::app_error::AppCommandError;
use crate::update::preferences;

#[derive(Debug)]
pub enum CatalogFetch {
    NotModified,
    Updated {
        snapshot: CatalogSnapshot,
        etag: Option<String>,
    },
}

pub(super) async fn fetch(
    conn: &DatabaseConnection,
    etag: Option<&str>,
) -> Result<CatalogFetch, AppCommandError> {
    let preferences = preferences::load(conn).await?;
    let agent_target = capability::current_agent_target();
    let fetched = fetch_target(
        conn,
        catalog_query(preferences.channel.as_str(), agent_target),
        etag,
    )
    .await?;
    let CatalogFetch::Updated { mut snapshot, etag } = fetched else {
        return Ok(fetched);
    };
    if agent_target != capability::current_target() {
        // Mac 的 Agent 与工具使用不同平台名，只合并同一目录版本的数据。
        let tools = fetch_target(
            conn,
            catalog_query(preferences.channel.as_str(), capability::current_target()),
            None,
        )
        .await?;
        let CatalogFetch::Updated {
            snapshot: tools, ..
        } = tools
        else {
            return Err(AppCommandError::network(
                "Managed tool catalog was not returned",
            ));
        };
        if snapshot.revision != tools.revision || snapshot.schema_version != tools.schema_version {
            tracing::warn!(
                agent_revision = snapshot.revision,
                tool_revision = tools.revision,
                "[AgentPlatform] catalog changed between Agent and tool queries"
            );
            return Err(AppCommandError::network(
                "Agent and tool catalog revisions differ",
            ));
        }
        snapshot.tools = tools.tools;
    }
    Ok(CatalogFetch::Updated { snapshot, etag })
}

async fn fetch_target(
    conn: &DatabaseConnection,
    query: [(&str, String); 5],
    etag: Option<&str>,
) -> Result<CatalogFetch, AppCommandError> {
    let mut request =
        AgentPlatformClient::request(conn, Method::GET, "/agent-platforms/v1/catalog")
            .await?
            .query(&query);
    if let Some(etag) = etag.filter(|value| !value.trim().is_empty()) {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let response = request.send().await.map_err(network_error)?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(CatalogFetch::NotModified);
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let snapshot = decode_response(response).await?;
    Ok(CatalogFetch::Updated { snapshot, etag })
}
