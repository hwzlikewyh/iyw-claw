pub(crate) fn patch_gateway_credentials(raw: &str, token: Option<&str>) -> Result<String, String> {
    let mut config: toml::Table = raw.parse().map_err(|error| format!("{error}"))?;
    let base =
        crate::acp::provider_overlay::model_gateway_base_url_for(crate::models::AgentType::Grok);
    if let Some(models) = config.get_mut("model").and_then(toml::Value::as_table_mut) {
        for (_, model) in models.iter_mut() {
            let Some(model) = model.as_table_mut() else {
                continue;
            };
            if model.get("base_url").and_then(toml::Value::as_str) == Some(base.as_str()) {
                set_token_header(model, token)?;
            }
        }
    }
    toml::to_string_pretty(&config).map_err(|error| error.to_string())
}

fn set_token_header(model: &mut toml::Table, token: Option<&str>) -> Result<(), String> {
    if token.is_none() && !model.contains_key("extra_headers") {
        return Ok(());
    }
    let headers = model
        .entry("extra_headers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or("Managed model extra_headers must be a table")?;
    headers.retain(|key, _| !key.eq_ignore_ascii_case("token"));
    if let Some(token) = token {
        headers.insert("token".into(), token.into());
    }
    Ok(())
}

pub(crate) fn preserve_gateway_token(previous: Option<&toml::Value>, model: &mut toml::Table) {
    let Some(previous) = previous.and_then(toml::Value::as_table) else {
        return;
    };
    if previous.get("base_url") != model.get("base_url") {
        return;
    }
    let token = previous
        .get("extra_headers")
        .and_then(toml::Value::as_table)
        .and_then(|headers| {
            headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("token"))
        })
        .and_then(|(_, value)| value.as_str());
    if let Some(token) = token {
        let mut headers = toml::Table::new();
        headers.insert("token".into(), token.into());
        model.insert("extra_headers".into(), toml::Value::Table(headers));
    }
}
