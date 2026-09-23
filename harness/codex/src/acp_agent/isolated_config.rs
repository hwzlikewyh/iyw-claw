use serde_json::{Map, Value};

/// 临时线程仍需有效 transport；整表覆盖时不能只留下 enabled 字段。
pub(super) fn disabled_mcp_servers(settings: &Value) -> Result<Value, String> {
    let Some(servers) = settings.get("mcp_servers").filter(|value| !value.is_null()) else {
        return Ok(Value::Object(Map::new()));
    };
    let servers = servers
        .as_object()
        .ok_or("Invalid MCP configuration snapshot")?;
    let mut disabled = Map::new();
    for (name, server) in servers {
        let mut server = server.clone();
        let fields = server
            .as_object_mut()
            .ok_or("Invalid MCP server snapshot")?;
        fields.insert("enabled".into(), Value::Bool(false));
        fields.insert("required".into(), Value::Bool(false));
        // config/read 序列化的可选字段可能为 null，TOML 中应省略而非转为空字符串。
        omit_null_fields(&mut server);
        serde_json::from_value::<codex_config::McpServerConfig>(server.clone())
            .map_err(|_| "Invalid MCP transport in isolated thread snapshot")?;
        disabled.insert(name.clone(), server);
    }
    Ok(Value::Object(disabled))
}

fn omit_null_fields(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.retain(|_, value| !value.is_null());
            for value in fields.values_mut() {
                omit_null_fields(value);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(omit_null_fields),
        _ => {}
    }
}
