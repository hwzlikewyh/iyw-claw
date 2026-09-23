use std::collections::BTreeMap;
use std::time::Instant;

use rmcp::ErrorData;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{failure, RemoteAccount, REMOTE_PREFIX};

const MAX_ROUTES: usize = 4096;

#[derive(Clone)]
pub(super) struct RemoteRoute {
    pub id: String,
    pub version: Option<String>,
    pub group: bool,
    accessed: Instant,
}

impl RemoteRoute {
    fn parse(value: &Value) -> Result<Self, ErrorData> {
        let id = value
            .get("tool_id")
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid("Directory item has no stable ID"))?;
        let group = match value.get("kind").and_then(Value::as_str) {
            Some("group") => true,
            Some("tool") => false,
            _ => return Err(invalid("Directory item has an unknown kind")),
        };
        let version = value
            .get("tool_version")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
        if !group && version.is_none() {
            return Err(invalid("Directory member has no tool_version"));
        }
        Ok(Self {
            id: id.to_string(),
            version: version.map(str::to_string),
            group,
            accessed: Instant::now(),
        })
    }

    fn capability_id(&self, account: &[u8; 32]) -> Result<String, ErrorData> {
        let encoded = serde_json::to_vec(&(account, &self.id, &self.version, self.group))
            .map_err(|_| invalid("Cannot encode directory identity"))?;
        Ok(format!("{REMOTE_PREFIX}{:x}", Sha256::digest(encoded)))
    }
}

impl RemoteAccount {
    pub(super) fn route(&self, capability_id: &str) -> Result<RemoteRoute, ErrorData> {
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let route = routes.get_mut(capability_id).ok_or_else(|| {
            failure(
                "remote_catalog_expired",
                "Remote ID is no longer in the current account catalog; search again",
                false,
            )
        })?;
        route.accessed = Instant::now();
        Ok(route.clone())
    }

    fn register(&self, value: &Value) -> Result<String, ErrorData> {
        let route = RemoteRoute::parse(value)?;
        let capability_id = route.capability_id(&self.fingerprint)?;
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !routes.contains_key(&capability_id) {
            evict_oldest(&mut routes);
        }
        routes.insert(capability_id.clone(), route);
        Ok(capability_id)
    }

    pub(super) fn summaries(&self, payload: &Value) -> Result<Vec<Value>, ErrorData> {
        payload
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Search response has no results array"))?
            .iter()
            .map(|item| self.project(item, false))
            .collect()
    }

    fn project(&self, item: &Value, full: bool) -> Result<Value, ErrorData> {
        let capability_id = self.register(item)?;
        let mut detail = json!({
            "capability_id": capability_id, "source": "remote", "status": "available",
            "kind": item.get("kind"), "name": item.get("name"), "tool_version": item.get("tool_version"),
        });
        if full {
            detail["description"] = item["description"].clone();
            detail["usage"] = item["usage"].clone();
            add_schema(&mut detail, item)?;
        } else {
            detail["summary"] = item["description"].clone();
            detail["required_inputs"] =
                item.get("required_arguments").cloned().unwrap_or(json!([]));
            detail["match_reason"] = item["match_reason"].clone();
        }
        Ok(detail)
    }

    pub(super) fn detail(
        &self,
        payload: &Value,
        requested: &RemoteRoute,
    ) -> Result<Value, ErrorData> {
        if payload.get("id").and_then(Value::as_str) != Some(requested.id.as_str()) {
            return Err(invalid("Read response does not match the requested ID"));
        }
        let items = payload
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Read response has no versioned members"))?;
        if !requested.group {
            let item = items
                .iter()
                .find(|item| {
                    item.get("tool_id").and_then(Value::as_str) == Some(requested.id.as_str())
                })
                .ok_or_else(|| invalid("Read response omits the requested member"))?;
            return self.project(item, true);
        }
        let members = items
            .iter()
            .map(|item| self.project(item, true))
            .collect::<Result<Vec<_>, _>>()?;
        let mut group = self.project(payload, false)?;
        group["description"] = payload["description"].clone();
        group.as_object_mut().map(|value| value.remove("summary"));
        group["items"] = json!(members);
        group["invocable"] = json!(false);
        group["guidance"] = json!("Read this workflow and relevant member schemas/usage. Invoke a member capability_id, never the group. A full member here needs no additional read.");
        Ok(group)
    }
}

fn add_schema(detail: &mut Value, item: &Value) -> Result<(), ErrorData> {
    let schema = item
        .pointer("/schema/inputSchema")
        .ok_or_else(|| invalid("Directory member has no input schema"))?;
    detail["input_schema"] = schema.clone();
    detail["required_inputs"] = json!(super::super::capability_metadata::required_inputs(schema));
    detail["schema_digest"] = json!(super::super::capability_metadata::digest(schema)
        .map_err(|_| invalid("Cannot encode remote schema"))?);
    for field in ["annotations", "outputSchema", "execution"] {
        if let Some(value) = item["schema"].get(field) {
            detail[field] = value.clone();
        }
    }
    Ok(())
}

fn evict_oldest(routes: &mut BTreeMap<String, RemoteRoute>) {
    if routes.len() < MAX_ROUTES {
        return;
    }
    let oldest = routes
        .iter()
        .min_by_key(|(_, route)| route.accessed)
        .map(|(id, _)| id.clone());
    if let Some(id) = oldest {
        routes.remove(&id);
    }
}

fn invalid(message: &str) -> ErrorData {
    failure("remote_catalog_invalid", message, false)
}
