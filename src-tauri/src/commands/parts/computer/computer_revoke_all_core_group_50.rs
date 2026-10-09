// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// Stop sharing every window.
pub fn computer_revoke_all_core(service: &ComputerService) {
    let ended = service.targets.revoke_all(GrantChange::Revoked);
    service.announce_change(ended);
}
