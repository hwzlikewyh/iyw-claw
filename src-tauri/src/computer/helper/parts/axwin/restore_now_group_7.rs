// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) fn restore_now<E>(
    pid: u32,
    window_id: u64,
    ready: impl Fn() -> Result<(), E>,
) -> Result<Restore, E> {
    let Some(app) = application(pid) else {
        return Ok(Restore::Failed(AX_FAILURE));
    };
    // A hidden application shows its windows again as a whole; a minimized
    // one among them stays in the Dock until it is asked for too.
    let hidden = flag(&app, "AXHidden") == Some(true);
    if hidden {
        ready()?;
        let err = set_false(&app, "AXHidden");
        if err != AX_SUCCESS {
            return Ok(Restore::Failed(err));
        }
    }
    let shown = if hidden {
        Restore::Asked
    } else {
        Restore::AlreadyShown
    };
    let windows = match windows(&app) {
        Ok(windows) => windows,
        Err(_) if hidden => return Ok(shown),
        Err(e) => return Ok(Restore::Failed(e)),
    };
    let Some(window) = windows
        .into_iter()
        .find(|w| window_number(w) == Some(window_id))
    else {
        return Ok(if hidden { shown } else { Restore::Unlisted });
    };
    if flag(&window, "AXMinimized") == Some(false) {
        return Ok(shown);
    }
    ready()?;
    let err = set_false(&window, "AXMinimized");
    Ok(if err == AX_SUCCESS {
        Restore::Asked
    } else {
        Restore::Failed(err)
    })
}

/// Set a yes-or-no attribute of `element` to no.
pub(super) fn set_false(element: &CFType, name: &'static str) -> AXError {
    let name = CFString::from_static_string(name);
    // SAFETY: a live element, a valid attribute name and a CFBoolean that
    // outlives the call.
    unsafe {
        AXUIElementSetAttributeValue(
            element.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            CFBoolean::false_value().as_CFTypeRef(),
        )
    }
}

/// `pid`'s application, as Accessibility sees it.
pub(super) fn application(pid: u32) -> Option<CFType> {
    let pid = libc::pid_t::try_from(pid).ok()?;
    // SAFETY: returns a +1 element, or null.
    let raw = unsafe { AXUIElementCreateApplication(pid) };
    if raw.is_null() {
        return None;
    }
    // SAFETY: the +1 element from above, released by the wrapper.
    let app = unsafe { CFType::wrap_under_create_rule(raw) };
    bound(&app);
    Some(app)
}

/// Hold every question to `element` to [`TIMEOUT`]. Set on each element
/// asked: the bound is the element's own, not its application's.
pub(super) fn bound(element: &CFType) {
    // SAFETY: a live element; this only sets a number on it.
    unsafe { AXUIElementSetMessagingTimeout(element.as_CFTypeRef(), TIMEOUT) };
}

/// One attribute's value, or the error the application answered with.
pub(super) fn attribute(element: &CFType, name: &'static str) -> Result<CFType, AXError> {
    let name = CFString::from_static_string(name);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: a live element, a valid attribute name, and an out pointer that
    // receives a +1 value on success.
    let err = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            &mut value,
        )
    };
    if err != AX_SUCCESS {
        return Err(err);
    }
    if value.is_null() {
        return Err(AX_NO_VALUE);
    }
    // SAFETY: the +1 value from above, released by the wrapper.
    Ok(unsafe { CFType::wrap_under_create_rule(value) })
}

/// A yes-or-no attribute. Some applications answer with a number.
pub(super) fn flag(element: &CFType, name: &'static str) -> Option<bool> {
    let value = attribute(element, name).ok()?;
    if let Some(yes) = value.downcast::<CFBoolean>() {
        return Some(yes.into());
    }
    value
        .downcast::<CFNumber>()
        .and_then(|n| n.to_i64())
        .map(|n| n != 0)
}

/// The application's windows, as Accessibility lists them.
pub(super) fn windows(app: &CFType) -> Result<Vec<CFType>, AXError> {
    elements(app, "AXWindows")
}

/// The elements an attribute of `element` lists.
pub(super) fn elements(element: &CFType, name: &'static str) -> Result<Vec<CFType>, AXError> {
    let list = attribute(element, name)?
        .downcast_into::<CFArray>()
        .ok_or(AX_NO_VALUE)?;
    // SAFETY: a pure query.
    let element_type = unsafe { AXUIElementGetTypeID() };
    Ok(list
        .iter()
        .map(|item| *item)
        // SAFETY: a non-null object the array holds.
        .filter(|item| !item.is_null() && unsafe { CFGetTypeID(*item) } == element_type)
        .map(|item| {
            // SAFETY: an element the array holds, retained for the wrapper.
            let window = unsafe { CFType::wrap_under_get_rule(item) };
            bound(&window);
            window
        })
        .collect())
}

/// The window server's number for an accessibility window: what the
/// driver's listing names windows by. Private (`_AXUIElementGetWindow`), and
/// what every window manager and the driver itself map windows with; looked
/// up at run time, so a macOS without it costs the helper this module, not
/// its start.
pub(super) fn window_number(window: &CFType) -> Option<u64> {
    type GetWindow = unsafe extern "C" fn(CFTypeRef, *mut u32) -> AXError;
    static GET_WINDOW: OnceLock<Option<GetWindow>> = OnceLock::new();
    let get = (*GET_WINDOW.get_or_init(|| {
        static NAME: &[u8] = b"_AXUIElementGetWindow\0";
        // SAFETY: RTLD_DEFAULT lookup of a NUL-terminated name; this module
        // links ApplicationServices, which carries it.
        let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, NAME.as_ptr().cast()) };
        // SAFETY: the symbol has exactly this signature.
        (!sym.is_null())
            .then(|| unsafe { std::mem::transmute::<*mut libc::c_void, GetWindow>(sym) })
    }))?;
    let mut number = 0u32;
    // SAFETY: a live window element and an out pointer for the number.
    let err = unsafe { get(window.as_CFTypeRef(), &mut number) };
    (err == AX_SUCCESS && number != 0).then_some(u64::from(number))
}
