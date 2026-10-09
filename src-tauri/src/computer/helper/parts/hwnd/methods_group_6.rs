// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl Object {
    /// The object's table of methods, read as `M`.
    ///
    /// # Safety
    ///
    /// `M` must lay out the start of the object's own table.
    pub(in crate::computer::helper::hwnd) unsafe fn methods<M>(&self) -> &M {
        &**self.0.cast::<*const M>()
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        // SAFETY: every table starts as IUnknown's; our one reference,
        // released once.
        unsafe { (self.methods::<UnknownMethods>().release)(self.0) };
    }
}
