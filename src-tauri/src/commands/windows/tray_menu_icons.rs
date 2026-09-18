use tauri::menu::{IconMenuItem, IconMenuItemBuilder};
use tauri::{image::Image, AppHandle};

use super::{
    TRAY_MENU_ID_NEW, TRAY_MENU_ID_OPEN, TRAY_MENU_ID_QUIT, TRAY_MENU_ID_RECENT_PREFIX,
    TRAY_MENU_ID_SETTINGS, TRAY_MENU_ID_UPDATE,
};

pub(super) fn menu_item(
    app: &AppHandle,
    id: &str,
    text: &str,
) -> tauri::Result<IconMenuItem<tauri::Wry>> {
    let bytes: &[u8] = match id {
        TRAY_MENU_ID_OPEN => include_bytes!("../../../icons/tray-menu/workspace.png"),
        TRAY_MENU_ID_NEW => include_bytes!("../../../icons/tray-menu/conversation.png"),
        TRAY_MENU_ID_SETTINGS => include_bytes!("../../../icons/tray-menu/settings.png"),
        TRAY_MENU_ID_UPDATE => include_bytes!("../../../icons/tray-menu/update.png"),
        TRAY_MENU_ID_QUIT => include_bytes!("../../../icons/tray-menu/quit.png"),
        _ => include_bytes!("../../../icons/tray-menu/folder.png"),
    };
    let text = if id.starts_with(TRAY_MENU_ID_RECENT_PREFIX) {
        project_label(text)
    } else {
        text.to_owned()
    };
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?
        .to_rgba8();
    let (width, height) = decoded.dimensions();
    IconMenuItemBuilder::with_id(id, text)
        .icon(Image::new_owned(decoded.into_raw(), width, height))
        .build(app)
}

fn project_label(name: &str) -> String {
    const MAX_LABEL_CHARACTERS: usize = 36;
    let normalized = name.replace(['\n', '\r', '\t'], " ");
    let mut label: String = normalized.chars().take(MAX_LABEL_CHARACTERS).collect();
    if normalized.chars().count() > MAX_LABEL_CHARACTERS {
        label.push_str("...");
    }
    label.replace('&', "&&")
}
