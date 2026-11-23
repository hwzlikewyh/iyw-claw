#[path = "plugin_names_data.rs"]
mod data;

pub(crate) struct Rename {
    pub id: i64,
    pub legacy: &'static str,
    pub name: &'static str,
}

pub(crate) fn migration(slug: &str) -> Option<&'static Rename> {
    data::RENAMES
        .iter()
        .find(|value| slug == value.legacy || slug == value.name)
}

pub(crate) fn alternate_name(slug: &str) -> Option<(i64, &'static str)> {
    migration(slug).map(|value| {
        (
            value.id,
            if slug == value.name {
                value.legacy
            } else {
                value.name
            },
        )
    })
}

pub(crate) fn same_plugin(id: i64, left: &str, right: &str) -> bool {
    left == right
        || migration(left)
            .is_some_and(|value| value.id == id && (right == value.legacy || right == value.name))
}

pub(crate) fn same_capability(id: i64, left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left = left
        .strip_prefix("plugin.")
        .and_then(|value| value.split_once('.'));
    let right = right
        .strip_prefix("plugin.")
        .and_then(|value| value.split_once('.'));
    match (left, right) {
        (Some((left_name, left_tool)), Some((right_name, right_tool))) => {
            left_tool == right_tool && same_plugin(id, left_name, right_name)
        }
        _ => false,
    }
}
