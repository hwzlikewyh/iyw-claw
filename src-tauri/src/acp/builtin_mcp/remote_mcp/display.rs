use rmcp::model::CallToolResult;
use serde_json::{json, Value};

use super::{RemoteAccount, RemoteGateway, RemoteRoute};

const MAX_NAME_CHARS: usize = 80;
const TOOL_NAME_RANK: u8 = 1;
const DISPLAY_NAME_RANK: u8 = 2;

fn readable_name(value: &str) -> Option<String> {
    let name: String = value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= MAX_NAME_CHARS).then(|| name.to_owned())
}

pub(super) fn catalog_name(value: &Value) -> Option<(u8, String)> {
    [
        (DISPLAY_NAME_RANK, "/display_name"),
        (DISPLAY_NAME_RANK, "/schema/annotations/title"),
        (DISPLAY_NAME_RANK, "/annotations/title"),
        (DISPLAY_NAME_RANK, "/schema/title"),
        (TOOL_NAME_RANK, "/name"),
    ]
    .into_iter()
    .find_map(|(rank, path)| readable_name(value.pointer(path)?.as_str()?).map(|name| (rank, name)))
}

impl RemoteGateway {
    pub(in crate::acp::builtin_mcp) async fn tool_display(&self, id: &str) -> Option<Value> {
        let current = self.account.lock().await;
        let account = current.as_ref()?;
        if self.shutdown.is_cancelled()
            || account.cancellation.is_cancelled()
            || account.credentials_cancel.is_cancelled()
        {
            return None;
        }
        let name = if let Some(name) = id.strip_prefix(super::direct::CAPABILITY_PREFIX) {
            let tool = account.resolve_direct(name).ok()?;
            tool.title
                .as_deref()
                .and_then(readable_name)
                .or_else(|| {
                    tool.annotations
                        .as_ref()?
                        .title
                        .as_deref()
                        .and_then(readable_name)
                })
                .or_else(|| readable_name(&tool.name))?
        } else {
            account.route(id).ok()?.display_name?.1
        };
        Some(json!({"capability_id": id, "name": name}))
    }
}

pub(super) fn annotate_result(
    result: &mut CallToolResult,
    identity: (&str, &RemoteRoute),
    account: &RemoteAccount,
) {
    let (id, route) = identity;
    let receipt_name = result
        .meta
        .as_ref()
        .and_then(|meta| meta.get("directory_tool"))
        .filter(|item| {
            item["tool_id"].as_str() == Some(route.id.as_str())
                && item["tool_version"].as_str() == route.version.as_deref()
        })
        .and_then(catalog_name);
    let name = receipt_name.or_else(|| route.display_name.clone());
    let Some(mut display) = name else { return };
    if let Some(cached) = account
        .routes
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get_mut(id)
    {
        if let Some(previous) = cached
            .display_name
            .as_ref()
            .filter(|(rank, _)| *rank > display.0)
        {
            display = previous.clone();
        }
        cached.display_name = Some(display.clone());
    }
    result.meta.get_or_insert_with(Default::default).insert(
        "iyw.toolDisplay".to_owned(),
        json!({"capability_id": id, "name": display.1}),
    );
}
