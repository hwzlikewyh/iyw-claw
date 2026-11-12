# Tao 0.34.5 Windows 会话结束修复

基于 crates.io 的 Tao 0.34.5，发布包 SHA-256：

```text
f3a753bdc39c07b192151523a3f77cd0394aa75413802c883a0f6f6a0e5ee2e7
```

保留上游源码与 Apache-2.0 许可证，仅回移 Windows 事件循环修复。
Cargo.toml 移除了本地未附带的 example 入口，依赖和 feature 定义保持一致。

## 修复

`src/platform_impl/windows/event_loop.rs` 在确认 `WM_ENDSESSION` 后，
发出 `LoopDestroyed` 并立即以状态码 0 退出。Windows Restart Manager 也会发出
这条消息，但会等待应用自行退出；原实现继续派发事件，会触发
`cannot move state from Destroyed` panic。

该行为回移自已合入的官方修复，不改变取消结束会话（`wParam=FALSE`）、
普通窗口关闭、应用主动退出或其他平台的行为。

- 官方 PR：https://github.com/tauri-apps/tao/pull/1157
- 官方提交：`36b31b795f71ea52528b0d65de86c54645f3634d`
- 官方维护者对根因的说明：https://github.com/tauri-apps/tao/pull/1188

应用在 `RunEvent::Exit` 同步记录 `event_loop_exit`，确保直接退出前仍有退出标记。
该路径遵循系统结束会话语义，不承诺执行普通异步退出流程。

## 移除条件

当项目升级到已包含官方 #1157 修复的兼容 Tauri/Tao 组合时，移除此本地覆盖，
重新生成锁文件，并核查正常退出、系统结束会话及 Restart Manager 场景。
