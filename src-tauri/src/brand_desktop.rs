use base64::Engine;
use tauri::{plugin::TauriPlugin, Context, Wry};

pub fn configure(mut context: Context<Wry>) -> Context<Wry> {
    context.config_mut().identifier = super::identifier().into();
    context.config_mut().product_name = Some(super::name().into());
    context.config_mut().plugins.0.insert("deep-link".into(),
        serde_json::json!({"desktop": {"schemes": [super::scheme()]}}));
    if let Some(bytes) = &super::current().logo {
        if let Ok(decoded) = image::load_from_memory_with_format(bytes, image::ImageFormat::Png) {
            let pixels = decoded.into_rgba8();
            let (width, height) = pixels.dimensions();
            context.set_default_window_icon(Some(tauri::image::Image::new_owned(pixels.into_raw(), width, height)));
        }
    }
    context
}

pub fn plugin() -> TauriPlugin<Wry> {
    let logo = super::current().logo.as_ref().map(|bytes|
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
    let data = serde_json::json!({
        "brandId": super::id(), "displayName": super::name(),
        "urlScheme": super::scheme(), "logoUrl": logo,
    });
    let script = format!(
        "Object.defineProperty(window,'__IYW_BRAND__',{{value:{data},writable:false,configurable:false}});"
    );
    tauri::plugin::Builder::new("installed-brand").js_init_script(script)
        .setup(|_, _| {
            #[cfg(target_os = "macos")]
            set_dock_icon();
            Ok(())
        }).build()
}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{class, msg_send, rc::{Allocated, Retained}, runtime::AnyObject};
    let Some(bytes) = &super::current().logo else { return; };
    // 插件 setup 在主线程执行，图标仅应用到当前进程。
    unsafe {
        let data: Retained<AnyObject> = msg_send![class!(NSData), dataWithBytes: bytes.as_ptr(), length: bytes.len()];
        let image: Allocated<AnyObject> = msg_send![class!(NSImage), alloc];
        let image: Option<Retained<AnyObject>> = msg_send![image, initWithData: &*data];
        if let Some(image) = image {
            let application: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![&*application, setApplicationIconImage: &*image];
        }
    }
}
