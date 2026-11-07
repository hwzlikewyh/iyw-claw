use super::plugin_types::SkillPluginComponent;
use crate::app_error::AppCommandError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[path = "plugin_setup_types.rs"]
mod types;
pub use types::{Authentication, Parameter, Preparation, ToolAction};

const MAX_PARAMETERS: usize = 32;
const MAX_ARGUMENT_BYTES: usize = 8192;

pub(crate) fn parse(raw: &Value) -> Result<Preparation, AppCommandError> {
    if null_declaration(raw) {
        return Err(invalid("插件准备声明不能包含null字段"));
    }
    let preparation: Preparation =
        serde_json::from_value(raw.clone()).map_err(|_| invalid("插件准备声明格式无效"))?;
    if preparation.parameters.len() > MAX_PARAMETERS {
        return Err(invalid("插件参数超过数量上限"));
    }
    let mut seen = BTreeSet::new();
    for field in &preparation.parameters {
        validate_parameter(field, &mut seen)?;
    }
    if let Some(auth) = &preparation.authentication {
        validate_auth(auth)?;
    }
    Ok(preparation)
}

fn null_declaration(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(values) => values.iter().any(|(key, value)| {
            if key == "arguments" {
                !value.is_object()
            } else {
                null_declaration(value)
            }
        }),
        Value::Array(values) => values.iter().any(null_declaration),
        _ => false,
    }
}

pub(crate) fn validate_components(
    components: &[SkillPluginComponent],
) -> Result<(), AppCommandError> {
    for connector in components
        .iter()
        .filter(|component| component.kind == "connector")
    {
        let Some(raw) = connector
            .config
            .as_ref()
            .and_then(|config| config.get("preparation"))
        else {
            continue;
        };
        let preparation = parse(raw)?;
        let Some(auth) = preparation.authentication else {
            continue;
        };
        for action in [&auth.status]
            .into_iter()
            .chain(auth.login.iter())
            .chain(auth.logout.iter())
        {
            let matches = components
                .iter()
                .filter(|component| component.kind == "capability")
                .filter_map(|component| component.config.as_ref())
                .filter(|config| {
                    config["connectorKey"] == connector.key
                        && config["toolName"] == action.tool_name
                })
                .collect::<Vec<_>>();
            if matches.is_empty()
                || (action.tool_name == auth.status.tool_name
                    && matches
                        .iter()
                        .any(|config| config["readOnlyHint"].as_bool() != Some(true)))
            {
                return Err(invalid("认证工具须由同一连接器声明，状态工具必须明确只读"));
            }
        }
    }
    Ok(())
}

fn validate_parameter(
    field: &Parameter,
    seen: &mut BTreeSet<String>,
) -> Result<(), AppCommandError> {
    if !safe_key(&field.key) || !seen.insert(field.key.clone()) {
        return Err(invalid("插件参数环境名称无效、保留或重复"));
    }
    if field.label.trim().is_empty()
        || field.label.len() > 160
        || field.description.len() > 2000
        || field.placeholder.len() > 512
    {
        return Err(invalid("插件参数标题或说明无效"));
    }
    if (field.secret && field.default_value.is_some())
        || (credential_key(&field.key) && !field.secret)
    {
        return Err(invalid("密钥参数必须标记敏感且不能有默认值"));
    }
    if field
        .default_value
        .as_ref()
        .is_some_and(|value| value.len() > 4096 || value.contains('\0'))
    {
        return Err(invalid("插件参数默认值无效"));
    }
    Ok(())
}

pub(crate) fn safe_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    if key.len() > 64
        || !bytes
            .next()
            .is_some_and(|value| value.is_ascii_alphabetic() || value == b'_')
        || !bytes.all(|value| value.is_ascii_alphanumeric() || value == b'_')
    {
        return false;
    }
    let upper = key.to_ascii_uppercase();
    if upper.ends_with("_DATA_DIR")
        || [
            "IYW_",
            "CODEX_",
            "CLAUDE_",
            "OPENAI_",
            "ANTHROPIC_",
            "AWS_",
            "AZURE_",
        ]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
    {
        return false;
    }
    ![
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "PATH",
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "SHELL",
        "NODE_OPTIONS",
        "NODE_PATH",
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
        "PYTHONPATH",
        "PYTHONHOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
    ]
    .contains(&upper.as_str())
}

fn credential_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "cookie",
        "authorization",
    ]
    .iter()
    .any(|part| key.contains(part))
}

fn validate_auth(auth: &Authentication) -> Result<(), AppCommandError> {
    if !valid_pointer(&auth.authenticated_pointer)
        || (!auth.authorization_url_pointer.is_empty()
            && (auth.login.is_none() || !valid_pointer(&auth.authorization_url_pointer)))
    {
        return Err(invalid("认证状态或登录地址需要有效 JSON Pointer"));
    }
    let mut seen = BTreeSet::new();
    for action in [&auth.status]
        .into_iter()
        .chain(auth.login.iter())
        .chain(auth.logout.iter())
    {
        if action.tool_name.trim().is_empty()
            || action.tool_name.len() > 128
            || !seen.insert(&action.tool_name)
        {
            return Err(invalid("认证工具名称无效或重复"));
        }
        let raw = serde_json::to_string(&action.arguments).map_err(|_| invalid("认证参数无效"))?;
        if raw.len() > MAX_ARGUMENT_BYTES
            || raw.contains("${")
            || contains_credentials(&Value::Object(action.arguments.clone()))
        {
            return Err(invalid("认证工具参数不能携带密钥、模板或过量内容"));
        }
    }
    Ok(())
}

fn valid_pointer(value: &str) -> bool {
    if !value.starts_with('/') || value.len() > 256 {
        return false;
    }
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch.is_control() || (ch == '~' && !matches!(chars.next(), Some('0' | '1'))) {
            return false;
        }
    }
    true
}

fn contains_credentials(value: &Value) -> bool {
    match value {
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| credential_key(key) || contains_credentials(value)),
        Value::Array(values) => values.iter().any(contains_credentials),
        _ => false,
    }
}

pub(crate) fn values(
    preparation: &Preparation,
    stored: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, AppCommandError> {
    let mut values = BTreeMap::new();
    for field in &preparation.parameters {
        let value = stored
            .get(&field.key)
            .or(field.default_value.as_ref())
            .cloned()
            .unwrap_or_default();
        if value.len() > 4096 || value.contains('\0') {
            return Err(invalid("插件参数值无效"));
        }
        if field.required && value.trim().is_empty() {
            return Err(invalid(format!("请先填写插件参数：{}", field.label)));
        }
        if !value.is_empty() {
            values.insert(field.key.clone(), value);
        }
    }
    Ok(values)
}

fn invalid(message: impl Into<String>) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
