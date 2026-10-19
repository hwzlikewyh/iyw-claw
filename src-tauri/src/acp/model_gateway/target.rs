use super::error::GatewayError;

pub(super) fn upstream_root(base: &str) -> String {
    base.trim_end_matches('/')
        .trim_end_matches("/v1")
        .trim_end_matches("/anthropic")
        .to_owned()
}

pub(super) fn resolve(base: &str, path: &str) -> Result<String, GatewayError> {
    let anthropic = path.starts_with("/anthropic/");
    let path = path.strip_prefix("/anthropic").unwrap_or(path);
    let path = path
        .strip_prefix("/v1/")
        .ok_or_else(|| GatewayError::input("Unsupported model endpoint"))?;
    let allowed = [
        "models",
        "chat/completions",
        "responses",
        "responses/compact",
        "messages",
        "messages/count_tokens",
        "images/generations",
        "images/edits",
        "images/input/prepare",
        "image-analysis",
    ];
    if !allowed.contains(&path) {
        return Err(GatewayError::input("Unsupported model endpoint"));
    }
    let root = upstream_root(base);
    let prefix = if anthropic { "anthropic/v1" } else { "v1" };
    Ok(format!("{root}/{prefix}/{path}"))
}

pub(super) fn model_base(base: &str, anthropic: bool) -> String {
    if anthropic {
        format!("{base}/anthropic")
    } else {
        format!("{base}/v1")
    }
}
