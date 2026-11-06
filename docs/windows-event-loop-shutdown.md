# Windows 事件循环结束会话修复

## 问题与行为

0.1.319 与 0.1.321 的日志分别在 2026-11-02 17:32:55、
2026-11-03 13:37:56 记录 Tao 0.34.5 主线程 panic：

```text
cannot move state from Destroyed
```

上游维护者确认，Windows Restart Manager 的 `WM_ENDSESSION` 会销毁事件循环，
但仍等待应用自行退出。旧实现返回后继续分发消息，会访问已销毁的事件循环。
官方通过 [Tao #1157](https://github.com/tauri-apps/tao/pull/1157) 修复该路径。

本项目保留当前 Tauri 与 Tao 版本，通过已有的 `[patch.crates-io]` 机制，
将官方修复回移到 `src-tauri/vendor/tao`。收到确认结束会话的消息后，
先发出事件循环退出通知，再以状态码 0 结束进程。
`wParam=FALSE`（取消结束会话）、普通窗口关闭和其他平台不改变行为。

应用同步写入 `event_loop_exit`，即使随后直接结束进程也保留退出标记。
该标记表示收到框架退出通知，不表示普通异步资源清理已经完成。
补丁来源、发布包摘要和移除条件见
[vendor 说明](../src-tauri/vendor/tao/PATCH.md)。

## 方案与边界

- 选择回移已合入的官方修复：范围仅限 Windows 系统结束会话路径。
- 升级整个 Tauri 运行时会引入不相关的平台和 API 变更，本次不采用。
- 应用层减少特定窗口操作不能覆盖 Tao 内部消息派发，本次不采用。

现有日志没有完整调用栈或系统事件，因此不能证明两次历史 panic 的最初消息
都来自 Restart Manager。该修复覆盖官方确认的相同 panic 路径；实际故障机仍需回归。
0.1.323 在 `tauri-build` 阶段中断的日志没有异常代码，原因尚未确认，
不将此次修复作为该启动问题已解决的证据。

## 验证

1. 校验原始 crates.io 发布包 SHA-256 与原 Cargo.lock 相同。
2. 核对 80 个 vendored 源码文件，只有 Windows `event_loop.rs` 包含补丁差异。
3. 用 `cargo tree --offline --locked -i tao --depth 2` 确认 Tauri 实际选用本地补丁。
4. 对桌面后端执行 `cargo check --offline --locked --lib --features tauri-runtime`。
5. 检查补丁格式、依赖锁文件、退出通知顺序以及取消结束会话分支。

以上核查通过；Windows x64 完整桌面后端编译检查退出码为 0。
隔离工作区通过临时目录引用复用现有构建资源，未修改项目构建配置。

遵循仓库约定，不新增或执行自动化测试。编译与静态核查不能替代实际故障机上的
Restart Manager、正常退出和启动回归。
