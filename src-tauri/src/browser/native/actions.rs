use serde_json::{json, Value};

pub(super) fn operate(id: &str, action: &str, values: &Value) -> Result<Value, String> {
    let view = super::state::view(id)?;
    match action {
        "navigate" | "reload" | "back" | "forward" | "stop" => navigate(id, action, values)?,
        "devtools" => view.open_devtools(),
        "zoom" => view
            .zoom(
                values["zoom"]
                    .as_f64()
                    .filter(|value| (0.25..=5.0).contains(value))
                    .ok_or("Invalid browser zoom")?,
            )
            .map_err(|error| error.to_string())?,
        "cookies" => return super::cookies::command(&view, values),
        "import_cookies" => return super::cookies::replace(&view, values),
        "set_cookie" => {
            super::cookies::set_value(&view, values)?;
            return Ok(json!({"success":true}));
        }
        "find" => find(id, values)?,
        "state" => {}
        _ => return Err(format!("Unsupported native browser operation: {action}")),
    }
    super::state::snapshot(id)
}

fn navigate(id: &str, action: &str, values: &Value) -> Result<(), String> {
    let view = super::state::view(id)?;
    if matches!(action, "back" | "forward") {
        let (back, forward) = crate::native_page::platform::history(&view);
        if (action == "back" && !back) || (action == "forward" && !forward) {
            return Ok(());
        }
    }
    let url = if action == "navigate" {
        Some(
            super::super::tab_actions::validated_url(
                values["url"].as_str().ok_or("Browser URL is required")?,
            )
            .map_err(|error| error.message)?,
        )
    } else {
        None
    };
    super::state::with_page(id, |page| {
        page.loading = action != "stop";
        if let Some(url) = &url {
            page.url = url.clone();
        }
        Ok(())
    })?;
    let result = match action {
        "navigate" => view
            .load_url(url.as_deref().ok_or("Browser URL is required")?)
            .map_err(|error| error.to_string()),
        "reload" => view.reload().map_err(|error| error.to_string()),
        _ => crate::native_page::platform::navigate(&view, action),
    };
    if result.is_err() {
        super::state::with_page(id, |page| {
            page.loading = false;
            Ok(())
        })?;
    }
    result
}

fn find(id: &str, values: &Value) -> Result<(), String> {
    let view = super::state::view(id)?;
    let query = values["query"].as_str().unwrap_or("");
    let repeat = super::state::with_page(id, |page| Ok(page.find_term == query))?;
    crate::native_page::find::search(
        &view,
        query,
        values["backwards"].as_bool().unwrap_or(false),
        repeat,
    )?;
    super::state::with_page(id, |page| {
        page.find_term = query.to_string();
        Ok(())
    })
}
