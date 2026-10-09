// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// A window's accessibility tree, with the values of anything that looks like
/// a secret taken out — and, for the helper to hold on to, what it knows of
/// each element that can be acted on (see [`SnapshotFacts`]).
///
/// It becomes the window's snapshot in the driver, which aims a point only
/// for a window whose snapshot holds a capture of it: so the window is
/// captured too, at its own size, and the picture thrown away. Where it
/// cannot be captured (no Screen Recording, or out of sight) the tree comes
/// back all the same. The walk has [`WALK_BUDGET_MS`]; past it the tree comes
/// back cut short, and says so.
pub async fn snapshot(
    driver: &DriverProc,
    pid: u32,
    window_id: u64,
    max_depth: Option<u32>,
    max_elements: Option<u32>,
    query: Option<String>,
    app_menus: bool,
) -> Result<(RawSnapshot, Option<SnapshotFacts>), HelperError> {
    let mut args = json!({
        "pid": pid,
        "window_id": window_id,
        "include_screenshot": true,
        "include_accessibility_tree": true,
        "max_image_dimension": 0,
        "timeout_ms": WALK_BUDGET_MS,
    });
    if let Some(depth) = max_depth.filter(|d| *d > 0) {
        args["max_depth"] = json!(depth);
    }
    if let Some(elements) = max_elements.filter(|e| *e > 0) {
        args["max_elements"] = json!(elements);
    }
    if let Some(query) = query.filter(|q| !q.trim().is_empty()) {
        args["query"] = json!(query);
    }
    let result = call(driver, "get_window_state", args, WINDOW_STATE_TIMEOUT).await?;
    let meta = structured("get_window_state", &result)?;
    let tree = meta
        .get("tree_markdown")
        .and_then(Value::as_str)
        .ok_or_else(|| HelperError::failed("get_window_state answered without a tree"))?;
    // Without its application's menu bars unless the application is shared
    // as a whole — and then without the Apple menu and the application menu:
    // not shown, and nothing in them can be acted on. By where the tree puts
    // them, and — for a window shared on its own — by their roles wherever
    // it puts them.
    let protected = if app_menus {
        protected_menu_titles(pid).await
    } else {
        None
    };
    // Where the titles cannot be had, nothing of the menu bars is kept.
    let keep = match &protected {
        Some(protected) => AppMenus::Own { protected },
        None => AppMenus::Withheld,
    };
    let (kept, withheld) = without_app_menus(tree, Dialect::current(), keep);
    let redacted = redact_secrets(&kept);
    let listed = meta.get("elements").map(|elements| {
        if Dialect::current() != Dialect::Mac {
            return elements.clone();
        }
        let in_reach = |element: &&Value| {
            let index = element.get("element_index").and_then(Value::as_u64);
            let role = element.get("role").and_then(Value::as_str).unwrap_or("");
            !index.is_some_and(|i| u32::try_from(i).is_ok_and(|i| withheld.contains(&i)))
                && (keep != AppMenus::Withheld || !APP_MENU_ROLES.contains(&role))
        };
        Value::Array(
            elements
                .as_array()
                .into_iter()
                .flatten()
                .filter(in_reach)
                .cloned()
                .collect(),
        )
    });
    // No id when the driver kept no snapshot of the window (it could not
    // match its accessibility surface): the tree is still worth reading, and
    // nothing in it can be acted on.
    let snapshot_id = string(meta, "snapshot_id");
    let (refs, elements) = element_refs(&redacted.nodes, listed.as_ref());
    let facts = snapshot_id.clone().map(|id| SnapshotFacts {
        snapshot_id: id,
        elements,
    });
    let raw = RawSnapshot {
        tree: redacted.tree,
        element_count: meta
            .get("element_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        truncated: meta
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || tree.contains("AX tree truncated"),
        degraded: string(meta, "degraded_reason"),
        window_bounds: meta.get("window_bounds").and_then(rect),
        title: string(meta, "window_title"),
        snapshot_id,
        refs,
    };
    Ok((raw, facts))
}
