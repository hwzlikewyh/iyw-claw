use std::path::Path;

use super::types::PluginInvokeError;

const MAX_URI_BYTES: usize = 2048;
const MAX_SEGMENTS: usize = 64;

pub(crate) fn allowed(
    root: &Path,
    connector: &str,
    binding: (&str, &str),
) -> Result<bool, PluginInvokeError> {
    let (app_uri, requested) = binding;
    if app_uri == requested {
        return Ok(true);
    }
    let Some(native) = super::native_compat::connector(root, connector)? else {
        return Ok(false);
    };
    if native
        .resources
        .iter()
        .any(|item| item["uri"].as_str() == Some(requested))
    {
        return Ok(true);
    }
    Ok(native.resource_templates.iter().any(|item| {
        item["uriTemplate"].as_str().is_some_and(|template| {
            namespace(template) == namespace(app_uri) && matches_template(template, requested)
        })
    }))
}

pub(super) fn valid_template(template: &str) -> bool {
    let Some(parts) = segments(template) else {
        return false;
    };
    parts.len() >= 2
        && !parts[0].contains(['{', '}'])
        && parts.iter().enumerate().all(|(index, part)| {
            if let Some(parameter) = part
                .strip_prefix('{')
                .and_then(|value| value.strip_suffix('}'))
            {
                let name = parameter.strip_prefix('+').unwrap_or(parameter);
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    && (!parameter.starts_with('+') || index + 1 == parts.len())
            } else {
                !part.contains(['{', '}'])
            }
        })
}

fn matches_template(template: &str, requested: &str) -> bool {
    if !valid_template(template) {
        return false;
    }
    let (Some(pattern), Some(actual)) = (segments(template), segments(requested)) else {
        return false;
    };
    if actual.iter().any(|part| part.contains(['{', '}'])) {
        return false;
    }
    for (index, part) in pattern.iter().enumerate() {
        let Some(value) = actual.get(index) else {
            return false;
        };
        if part.starts_with("{+") {
            return true;
        }
        if !part.starts_with('{') && part != value {
            return false;
        }
    }
    pattern.len() == actual.len()
}

fn segments(uri: &str) -> Option<Vec<&str>> {
    if uri.len() > MAX_URI_BYTES || !uri.is_ascii() {
        return None;
    }
    let path = uri.strip_prefix("ui://")?;
    if path
        .bytes()
        .any(|byte| !byte.is_ascii_graphic() || b"%?#\\".contains(&byte))
    {
        return None;
    }
    let parts: Vec<_> = path.split('/').collect();
    (parts.len() <= MAX_SEGMENTS
        && parts
            .iter()
            .all(|part| !part.is_empty() && !matches!(*part, "." | "..")))
    .then_some(parts)
}

fn namespace(uri: &str) -> Option<&str> {
    uri.strip_prefix("ui://")?.split('/').next()
}
