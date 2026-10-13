use base64::Engine;
use serde_json::{json, Value};
use tauri_runtime_wry::wry::WebView;

type Callback = Box<dyn FnOnce(Result<Value, String>) + Send>;
const MAX_CAPTURE_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn start(view: &WebView, done: Callback) -> Result<(), String> {
    platform_capture(view, Box::new(move |result| done(result.and_then(encode))))
}

fn encode(bytes: Vec<u8>) -> Result<Value, String> {
    if bytes.len() > MAX_CAPTURE_BYTES {
        return Err("Screenshot exceeds the supported size".into());
    }
    Ok(
        json!({"data": base64::engine::general_purpose::STANDARD.encode(bytes), "mimeType": "image/png"}),
    )
}

type BytesCallback = Box<dyn FnOnce(Result<Vec<u8>, String>) + Send>;

#[cfg(target_os = "windows")]
fn platform_capture(view: &WebView, done: BytesCallback) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtWindows;
    use webview2_com::{
        CapturePreviewCompletedHandler,
        Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
    };
    use windows::Win32::UI::Shell::SHCreateMemStream;
    let stream =
        unsafe { SHCreateMemStream(None) }.ok_or("Could not allocate screenshot buffer")?;
    let filled = stream.clone();
    let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
        done(
            result
                .map_err(|error| error.to_string())
                .and_then(|_| read_stream(&filled)),
        );
        Ok(())
    }));
    unsafe {
        view.webview().CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &handler,
        )
    }
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn read_stream(stream: &windows::Win32::System::Com::IStream) -> Result<Vec<u8>, String> {
    use windows::Win32::System::Com::STREAM_SEEK_SET;
    const BUFFER_BYTES: usize = 64 * 1024;
    unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }.map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut buffer = [0; BUFFER_BYTES];
    loop {
        let mut read = 0;
        unsafe {
            stream.Read(
                buffer.as_mut_ptr().cast(),
                BUFFER_BYTES as u32,
                Some(&mut read),
            )
        }
        .ok()
        .map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(bytes);
        }
        if bytes.len() + read as usize > MAX_CAPTURE_BYTES {
            return Err("Screenshot exceeds the supported size".into());
        }
        bytes.extend_from_slice(&buffer[..read as usize]);
    }
}

#[cfg(target_os = "macos")]
fn platform_capture(view: &WebView, done: BytesCallback) -> Result<(), String> {
    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use objc2_web_kit::WKSnapshotConfiguration;
    use tauri_runtime_wry::wry::WebViewExtMacOS;
    let mtm = MainThreadMarker::new().ok_or("Snapshot must run on the main thread")?;
    let done = std::sync::Mutex::new(Some(done));
    let block = RcBlock::<dyn Fn(*mut NSImage, *mut NSError)>::new(
        move |image: *mut NSImage, error: *mut NSError| {
            let result = unsafe {
                if !error.is_null() {
                    Err((*error).localizedDescription().to_string())
                } else if image.is_null() {
                    Err("Snapshot returned no image".into())
                } else {
                    (*image)
                        .TIFFRepresentation()
                        .and_then(|tiff| NSBitmapImageRep::imageRepWithData(&tiff))
                        .and_then(|rep| {
                            rep.representationUsingType_properties(
                                NSBitmapImageFileType::PNG,
                                &NSDictionary::new(),
                            )
                        })
                        .map(|png| png.to_vec())
                        .ok_or("PNG encoding failed".into())
                }
            };
            if let Some(done) = done.lock().ok().and_then(|mut done| done.take()) {
                done(result);
            }
        },
    );
    unsafe {
        view.webview()
            .takeSnapshotWithConfiguration_completionHandler(
                Some(&WKSnapshotConfiguration::new(mtm)),
                &block,
            );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_capture(view: &WebView, done: BytesCallback) -> Result<(), String> {
    use tauri_runtime_wry::wry::WebViewExtUnix;
    use webkit2gtk::{prelude::WebViewExt, SnapshotOptions, SnapshotRegion};
    view.webview().snapshot(
        SnapshotRegion::Visible,
        SnapshotOptions::NONE,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            done(
                result
                    .map_err(|error| error.to_string())
                    .and_then(|surface| {
                        let image = gtk::cairo::ImageSurface::try_from(surface.clone())
                            .map_err(|error| error.to_string())?;
                        gdk::pixbuf_get_from_surface(&surface, 0, 0, image.width(), image.height())
                            .ok_or("Snapshot returned no pixels".to_string())?
                            .save_to_bufferv("png", &[])
                            .map_err(|error| error.to_string())
                    }),
            );
        },
    );
    Ok(())
}
