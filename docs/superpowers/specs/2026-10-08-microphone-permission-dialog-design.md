# 麦克风权限与错误 Dialog 设计

## 目标

Windows 桌面版点击语音输入后不显示 WebView2 的 `tauri.localhost` 麦克风授权气泡；
权限失败或没有输入设备时，使用 Claw 自己的应用内 dialog 展示可理解的错误信息。
语音请求仍必须由用户点击麦克风按钮触发，不能在启动时自动打开设备。

## 范围与边界

- 仅修改 Tauri Windows 桌面运行时和聊天输入框的语音错误展示。
- 浏览器/服务端模式继续使用浏览器自身的麦克风权限机制。
- 只自动允许主应用 WebView 的麦克风权限请求；其他来源不自动授权。
- 不改 Fusion、百炼模型、实时 WebSocket 协议和录音数据流。
- 当前系统不存在可用麦克风时，自动授权不会伪造设备，应用仍显示设备错误 dialog。

## 方案

主窗口创建后通过 Tauri `with_webview` 取得 WebView2 控制器，在 Windows 原生层注册
`PermissionRequested` 回调。回调读取请求来源和权限类型，仅当权限类型为麦克风且来源
为 Claw 自己的应用页面时，将状态设为 `ALLOW`；其他权限保持 WebView2 默认处理。
回调对象由 WebView2 持有，随窗口生命周期释放，注册失败记录一次 warning 并保留默认权限行为。

前端语音启动失败时不再调用 toast，而是将错误分类为权限拒绝、无设备/不支持、设备
占用或服务不可用，打开现有 Dialog 组件。Dialog 使用现有 i18n，提供关闭和重新尝试
语音输入的动作；不显示 Tauri、WebView2 或内部实现名。系统设置跳转只在已有跨平台
打开入口可安全使用时提供，否则只给出平台无关的处理提示。

## 数据流

1. 用户点击麦克风按钮。
2. 前端调用 `getUserMedia`；Windows 原生回调静默处理应用来源的麦克风授权。
3. 成功后继续现有 PCM 捕获和 Fusion 实时会话。
4. 失败后将 DOMException 名称映射为稳定的语音错误类型，Dialog 展示对应文案。
5. 用户点击重试时关闭当前错误状态并重新执行既有启动流程。

## 错误处理

- `NotAllowedError`/`SecurityError`：显示麦克风权限未开启。
- `NotFoundError`/`NotSupportedError`：显示未找到可用麦克风或设备不支持。
- `NotReadableError`：显示麦克风被其他程序占用或驱动不可用。
- 其他前端或 Fusion 错误：显示语音服务暂不可用。
- 原生权限回调失败不阻塞应用启动，仍允许 WebView2 使用默认权限流程。

## 验证

- `cargo fmt --check` 与 `cargo check --locked --offline --lib --jobs 2`。
- 前端 ESLint/类型检查覆盖语音错误 Dialog 和错误分类。
- 静态检查确认只对麦克风权限、只对应用来源设置 `ALLOW`。
- Windows 桌面手工验证：首次点击不出现 `tauri.localhost` 气泡；无设备时显示应用内
  dialog；有设备时仍能建立 Fusion 实时会话。

