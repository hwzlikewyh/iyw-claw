/// 纯 C ABI 身份接口不创建运行时；加载器在执行 worker 前核对。
const ABI_VERSION: u64 = 1;
const CORE_VERSION: u64 = 153 * 1_000 + 4;

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
    *b"IYW_XINGHE_WORKER|1|0.153.4|3d2ee51ca2d5db578f328aa75e20aa22c0197c9a|END_WORKER_ID\0";
