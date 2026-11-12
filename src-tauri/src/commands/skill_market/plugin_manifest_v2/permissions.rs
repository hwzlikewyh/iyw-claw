use super::*;

pub(super) fn validate(value: &SkillPluginPermissions) -> Result<(), AppCommandError> {
    for path in value.workspace.read.iter().chain(&value.workspace.write) {
        validate_path(path)?;
    }
    for domain in value
        .network
        .connect_domains
        .iter()
        .chain(&value.network.resource_domains)
        .chain(&value.network.frame_domains)
        .chain(&value.network.base_uri_domains)
    {
        validate_origin(domain)?;
    }
    if value.host.iter().any(|item| {
        !matches!(
            item.as_str(),
            "send-message"
                | "clipboard-write"
                | "open-link"
                | "camera"
                | "microphone"
                | "geolocation"
                | "model-gateway"
        )
    }) {
        return Err(invalid_plugin("Plugin v2 host permission is unsupported"));
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), AppCommandError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid_plugin("Plugin v2 workspace permission is unsafe"));
    }
    Ok(())
}

fn validate_origin(domain: &str) -> Result<(), AppCommandError> {
    let parsed = reqwest::Url::parse(domain).ok();
    if domain
        .chars()
        .any(|character| character.is_control() || character.is_whitespace() || character == '\\')
        || parsed.as_ref().is_none_or(|url| {
            url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !matches!(url.path(), "" | "/")
        })
    {
        return Err(invalid_plugin(
            "Plugin v2 network permission must use HTTPS",
        ));
    }
    Ok(())
}
