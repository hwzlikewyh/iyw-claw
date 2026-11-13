use serde_json::{json, Map};

use super::super::{request::FallbackRequest, ImageRequest};

const MAX_PLATFORM_INPUTS: usize = 10;
const SERIES_TERMS: &[&str] = &[
    "系列",
    "延展",
    "延伸",
    "延申",
    "四宫格",
    "4宫格",
    "四格",
    "extend",
    "series",
    "four-panel",
    "four panel",
    "4-panel",
    "4 panel",
    "2x2",
    "2 x 2",
    "2×2",
];

pub(in crate::acp::builtin_mcp::iyw_image) fn automatic_platform_kind(
    request: &ImageRequest,
    kind: &str,
) -> Option<&'static str> {
    // 模型锁定、蒙版和特定参数不能在切换平台时丢失；这类请求保留显式备选链。
    if !matches!(kind, "generate" | "edit")
        || !request.fallbacks.is_empty()
        || request.parameters.values().any(|value| !value.is_null())
        || request.parameters.contains_key("n")
        || request.parameters.contains_key("batchSize")
        || request.images.len() > MAX_PLATFORM_INPUTS
        || request.images.iter().any(|image| image.role() == "mask")
    {
        return None;
    }
    if kind == "generate" {
        return Some("micro-generate");
    }
    if request.images.len() > 1 {
        return Some("mix");
    }
    let prompt = request
        .prompt
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    Some(if SERIES_TERMS.iter().any(|term| prompt.contains(term)) {
        "extend"
    } else {
        "variation"
    })
}

pub(super) fn candidates(request: &ImageRequest, kind: &str) -> Vec<FallbackRequest> {
    let Some(kind) = automatic_platform_kind(request, kind) else {
        return request.fallbacks.clone();
    };
    let mut parameters = Map::new();
    // 每次执行只生产一张；count 仍由批处理层控制。
    parameters.insert("batchSize".into(), json!(1));
    tracing::info!(
        fallback_type = kind,
        "[iyw-image] platform fallback planned"
    );
    vec![FallbackRequest {
        kind: kind.into(),
        parameters,
        wait: None,
    }]
}
