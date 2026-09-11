use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::{UpstreamClient, UpstreamError};

const MODEL_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Default)]
pub(super) struct ModelSettings {
    pub current: Option<String>,
    pub effort: Option<Value>,
    models: Vec<Value>,
    fast: bool,
    service_tier: Option<Value>,
}

#[derive(Debug, Clone)]
pub(super) struct ModelSelection {
    model: String,
    effort: Option<Value>,
    fast: bool,
    service_tier: Option<Value>,
}

impl ModelSettings {
    pub(super) fn capture(&mut self, response: &Value) {
        if let Some(model) = response.get("model").and_then(Value::as_str) {
            self.current = Some(model.to_string());
        }
        self.effort = response
            .get("reasoningEffort")
            .or_else(|| response.get("effort"))
            .filter(|value| !value.is_null())
            .cloned();
        if let Some(tier) = response.get("serviceTier") {
            self.fast = tier.as_str() == Some("fast");
            self.service_tier = (!tier.is_null()).then(|| tier.clone());
        }
    }

    pub(super) fn apply(&mut self, selection: ModelSelection) {
        self.current = Some(selection.model);
        self.effort = selection.effort;
        self.fast = selection.fast;
        if let Some(tier) = selection.service_tier {
            self.service_tier = (!tier.is_null()).then_some(tier);
        }
    }

    pub(super) fn current_values(&self) -> Value {
        json!({ "model": self.current, "effort": self.effort, "serviceTier": self.service_tier })
    }

    pub(super) async fn load(&mut self, upstream: &UpstreamClient) -> Result<(), UpstreamError> {
        let mut models = Vec::new();
        let mut cursor = Value::Null;
        let mut visited = BTreeSet::new();
        loop {
            let page = upstream
                .request_global_json(json!({
                    "method": "model/list",
                    "params": { "cursor": cursor, "limit": MODEL_PAGE_SIZE },
                }))
                .await?;
            let entries = page.get("data").and_then(Value::as_array).ok_or_else(|| {
                UpstreamError::InvalidResponse("model/list has no model entries".into())
            })?;
            models.extend(entries.iter().cloned());
            let Some(next) = page.get("nextCursor").and_then(Value::as_str) else {
                self.models = models;
                return Ok(());
            };
            if !visited.insert(next.to_string()) {
                return Err(UpstreamError::InvalidResponse(
                    "model/list repeated its pagination cursor".into(),
                ));
            }
            cursor = Value::String(next.to_string());
        }
    }

    pub(super) fn request(
        &self,
        thread_id: &str,
        option: (&str, &str),
    ) -> Result<(Value, ModelSelection), UpstreamError> {
        let (config_id, value) = option;
        let mut selection = match config_id {
            "model" => self.select_model(value)?,
            "reasoning_effort" => self.select_effort(value)?,
            "fast-mode" => self.select_fast(value)?,
            _ => return Err(invalid_option(config_id)),
        };
        let mut params = json!({ "threadId": thread_id, "model": selection.model });
        if let Some(effort) = &selection.effort {
            params["effort"] = effort.clone();
        }
        if config_id == "fast-mode" || (self.fast && !selection.fast) {
            params["serviceTier"] = if selection.fast {
                json!("fast")
            } else {
                Value::Null
            };
            selection.service_tier = Some(params["serviceTier"].clone());
        }
        Ok((
            json!({ "method": "thread/settings/update", "params": params }),
            selection,
        ))
    }

    pub(super) fn options(&self) -> Vec<Value> {
        let Some(current) = self.current.as_deref() else {
            return Vec::new();
        };
        let mut models: Vec<Value> = self.models.iter().filter_map(model_option).collect();
        if !models.iter().any(|option| option["value"] == current) {
            models.insert(0, json!({ "value": current, "name": current }));
        }
        let mut options = vec![json!({
            "id": "model", "name": "模型", "category": "model", "type": "select",
            "currentValue": current, "options": models,
        })];
        if let Some(model) = self.find_model(current) {
            if let Some(option) = self.effort_option(model) {
                options.push(option);
            }
            if super::fast_mode::supported(Some(model)) {
                options.push(super::fast_mode::option(self.fast));
            }
        }
        options
    }

    pub(super) fn accepts_images(&self) -> bool {
        self.current
            .as_deref()
            .and_then(|id| self.find_model(id))
            .and_then(|model| model.get("inputModalities"))
            .and_then(Value::as_array)
            .is_none_or(|modalities| modalities.iter().any(|modality| modality == "image"))
    }

    fn find_model(&self, id: &str) -> Option<&Value> {
        self.models
            .iter()
            .find(|model| model["id"] == id || model["model"] == id)
    }

    fn select_model(&self, id: &str) -> Result<ModelSelection, UpstreamError> {
        let Some(model) = self.find_model(id) else {
            return self
                .current
                .as_deref()
                .filter(|current| *current == id)
                .map(|_| ModelSelection {
                    service_tier: None,
                    model: id.into(),
                    effort: self.effort.clone(),
                    fast: self.fast,
                })
                .ok_or_else(|| invalid_option("model"));
        };
        let effort = self
            .effort
            .as_ref()
            .filter(|effort| supports_effort(model, effort))
            .cloned()
            .or_else(|| model.get("defaultReasoningEffort").cloned());
        Ok(ModelSelection {
            service_tier: None,
            model: model
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .into(),
            effort,
            fast: self.fast && super::fast_mode::supported(Some(model)),
        })
    }

    fn select_effort(&self, effort: &str) -> Result<ModelSelection, UpstreamError> {
        let current = self
            .current
            .as_deref()
            .ok_or_else(|| invalid_option("model"))?;
        let model = self
            .find_model(current)
            .ok_or_else(|| invalid_option("model"))?;
        let effort = Value::String(effort.to_string());
        if !supports_effort(model, &effort) {
            return Err(invalid_option("reasoning_effort"));
        }
        Ok(ModelSelection {
            service_tier: None,
            model: current.into(),
            effort: Some(effort),
            fast: self.fast,
        })
    }

    fn select_fast(&self, value: &str) -> Result<ModelSelection, UpstreamError> {
        let current = self
            .current
            .as_deref()
            .ok_or_else(|| invalid_option("model"))?;
        if !matches!(value, "on" | "off")
            || (value == "on" && !super::fast_mode::supported(self.find_model(current)))
        {
            return Err(invalid_option("fast-mode"));
        }
        Ok(ModelSelection {
            service_tier: None,
            model: current.into(),
            effort: self.effort.clone(),
            fast: value == "on",
        })
    }

    fn effort_option(&self, model: &Value) -> Option<Value> {
        let efforts = model.get("supportedReasoningEfforts")?.as_array()?;
        if efforts.is_empty() {
            return None;
        }
        let current = self
            .effort
            .as_ref()
            .or_else(|| model.get("defaultReasoningEffort"))?;
        let options: Vec<Value> = efforts.iter().filter_map(|option| {
            let effort = option.get("reasoningEffort")?.as_str()?;
            Some(json!({ "value": effort, "name": effort, "description": option.get("description") }))
        }).collect();
        Some(json!({
            "id": "reasoning_effort", "name": "推理强度", "category": "thought_level",
            "type": "select", "currentValue": current, "options": options,
        }))
    }
}

fn supports_effort(model: &Value, effort: &Value) -> bool {
    model
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array)
        .is_some_and(|options| {
            options
                .iter()
                .any(|option| option.get("reasoningEffort") == Some(effort))
        })
}

fn model_option(model: &Value) -> Option<Value> {
    let id = model.get("model").or_else(|| model.get("id"))?.as_str()?;
    Some(json!({
        "value": id,
        "name": model.get("displayName").and_then(Value::as_str).unwrap_or(id),
        "description": model.get("description"),
    }))
}

fn invalid_option(option: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(format!("unsupported 星河 setting: {option}"))
}
