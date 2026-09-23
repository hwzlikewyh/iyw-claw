/// 纯 C ABI 身份接口不创建运行时；加载器在执行 worker 前核对。
const ABI_VERSION: u64 = 1;
const CORE_VERSION: u64 = 156 * 1_000 + 1;

#[no_mangle]
pub extern "C" fn iyw_xinghe_worker_abi_version() -> u64 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn iyw_xinghe_worker_core_version() -> u64 {
    CORE_VERSION
}

#[used]
#[no_mangle]
pub static IYW_XINGHE_WORKER_IDENTITY_V1: [u8; 83] =
    *b"IYW_XINGHE_WORKER|1|0.156.1|b412ff32c417f855c2b2d1581b77058eed87c84b|END_WORKER_ID\0";
