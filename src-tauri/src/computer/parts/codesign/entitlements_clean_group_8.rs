// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl CodeInfo {
    /// Refuse a signature carrying any entitlement that would let another
    /// process put code inside this one.
    pub fn entitlements_clean(&self) -> Result<(), String> {
        match self
            .entitlements
            .iter()
            .find(|e| DENIED_ENTITLEMENTS.contains(&e.as_str()))
        {
            Some(denied) => Err(format!("its signature carries {denied}")),
            None => Ok(()),
        }
    }
}
