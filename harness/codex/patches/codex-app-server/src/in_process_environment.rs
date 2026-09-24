use std::collections::HashMap;
use std::io;

use codex_config::types::McpServerTransportConfig;
use codex_core::config::Config;
use codex_protocol::shell_environment::create_env_from_vars;

/// 宿主环境只保存在当前运行时，配置重载时重新应用，不写入配置层或进程全局。
pub fn apply_host_environment(
    config: &mut Config,
    environment: &HashMap<String, String>,
) -> io::Result<()> {
    if environment.is_empty() {
        return Ok(());
    }
    let policy = &mut config.permissions.shell_environment_policy;
    let mut inherited = policy.clone();
    inherited.r#set.clear();
    let filtered = create_env_from_vars(environment.clone(), &inherited, None);
    for (key, value) in filtered {
        insert_inherited(&mut policy.r#set, &key, &value);
    }
    let mut servers = config.mcp_servers.get().clone();
    for server in servers
        .values_mut()
        .filter(|server| server.is_local_environment())
    {
        match &mut server.transport {
            McpServerTransportConfig::Stdio { env, env_vars, .. } => {
                let values = env.get_or_insert_with(HashMap::new);
                for (key, value) in environment {
                    if key.eq_ignore_ascii_case("PATH")
                        || env_vars
                            .iter()
                            .any(|variable| !variable.is_remote_source() && variable.name() == key)
                    {
                        insert_inherited(values, key, value);
                    }
                }
            }
            McpServerTransportConfig::StreamableHttp {
                http_headers,
                env_http_headers,
                bearer_token_env_var,
                ..
            } => bind_http_environment(
                (http_headers, env_http_headers, bearer_token_env_var),
                environment,
            ),
        }
    }
    config.mcp_servers.set(servers).map_err(io::Error::other)
}

fn insert_inherited(values: &mut HashMap<String, String>, key: &str, value: &str) {
    let present = values
        .keys()
        .any(|existing| existing == key || (cfg!(windows) && existing.eq_ignore_ascii_case(key)));
    if !present {
        values.insert(key.to_owned(), value.to_owned());
    }
}

fn bind_http_environment(
    headers: (
        &mut Option<HashMap<String, String>>,
        &mut Option<HashMap<String, String>>,
        &mut Option<String>,
    ),
    environment: &HashMap<String, String>,
) {
    let (literal, references, bearer) = headers;
    if let Some(references) = references {
        references.retain(|name, variable| {
            let Some(value) = environment.get(variable) else {
                return true;
            };
            let literal = literal.get_or_insert_with(HashMap::new);
            literal.retain(|key, _| !key.eq_ignore_ascii_case(name));
            literal.insert(name.clone(), value.clone());
            false
        });
    }
    if let Some(value) = bearer.as_ref().and_then(|name| environment.get(name)) {
        let literal = literal.get_or_insert_with(HashMap::new);
        literal.retain(|key, _| !key.eq_ignore_ascii_case("authorization"));
        literal.insert("Authorization".into(), format!("Bearer {value}"));
        *bearer = None;
    }
}
