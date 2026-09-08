// 上游文件沙箱只传入白名单环境；Windows 还会复制程序。此入口不依赖活动标记或 DLL。
fn main() {
    if !iyw_codex_harness::dispatch_upstream_helper() {
        eprintln!("星河辅助程序只能由内置运行时调用");
        std::process::exit(64);
    }
}
