// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Rebuild the caller's predicates in the driver's vocabulary. Every field is
/// copied by name from the closed types in `computer::types`, so nothing the
/// agent wrote reaches the driver unexamined.
pub fn driver_predicates(expect: &[VerifyPredicate]) -> Result<Vec<Value>, HelperError> {
    if expect.is_empty() || expect.len() > MAX_VERIFY_PREDICATES {
        return Err(HelperError::new(
            HelperErrorCode::BadRequest,
            format!("verify takes 1 to {MAX_VERIFY_PREDICATES} predicates"),
        ));
    }
    expect
        .iter()
        .map(|p| {
            let mut out = Map::new();
            if let Some(window) = &p.window {
                let mut w = Map::new();
                if let Some(exists) = window.exists {
                    w.insert("exists".into(), json!(exists));
                }
                if let Some(b) = &window.bounds {
                    let mut bounds = json!({ "x": b.x, "y": b.y, "width": b.width, "height": b.height });
                    if let Some(t) = b.tolerance_px {
                        bounds["tolerance_px"] = json!(t.clamp(0.0, 100.0));
                    }
                    w.insert("bounds".into(), bounds);
                }
                out.insert("window".into(), Value::Object(w));
            }
            if let Some(element) = &p.element {
                if element.exists == Some(false) {
                    return Err(HelperError::new(
                        HelperErrorCode::BadRequest,
                        "an element's absence cannot be proven; check `exists: true` or leave it out",
                    ));
                }
                let mut selector = Map::new();
                if let Some(role) = element.selector.role.as_deref().filter(|s| !s.is_empty()) {
                    selector.insert("role".into(), json!(role));
                }
                if let Some(label) = element.selector.label_contains.as_deref().filter(|s| !s.is_empty()) {
                    selector.insert("label_contains".into(), json!(label));
                }
                let mut e = Map::new();
                e.insert("selector".into(), Value::Object(selector));
                for (key, value) in [
                    ("exists", element.exists),
                    ("enabled", element.enabled),
                    ("selected", element.selected),
                ] {
                    if let Some(v) = value {
                        e.insert(key.into(), json!(v));
                    }
                }
                out.insert("element".into(), Value::Object(e));
            }
            if out.is_empty() {
                return Err(HelperError::new(
                    HelperErrorCode::BadRequest,
                    "every predicate needs a `window` or an `element` part",
                ));
            }
            Ok(Value::Object(out))
        })
        .collect()
}
