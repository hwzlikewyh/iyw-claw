# 实时语音 WebSocket Upgrade 修复设计

## 目标

修复桌面端实时语音无法启动的问题。语音客户端必须在 HTTP Upgrade 阶段携带登录
`token`，同时保留连接后的实时语音 auth 首帧，使请求同时满足 Fusion 网关中间件和
实时语音协议。

## 问题

安装版 `0.1.92` 通过 URL 直接生成标准 WebSocket 握手，但 Upgrade 请求没有
`token`。当前生产网关会在进入实时语音 handler 前校验请求头，因此返回普通 HTTP
业务错误，客户端无法得到 `101 Switching Protocols`。

当前源码已尝试把 `token` 加到 Upgrade 请求，却改为通过 `Request::builder()` 手工
创建请求。`tokio-tungstenite 0.26` 对现成的 `Request` 不会补充 `Host`、`Connection`、
`Upgrade`、`Sec-WebSocket-Version` 和 `Sec-WebSocket-Key`，所以该请求仍不能完成
标准握手。

## 方案

在 `src-tauri/src/commands/realtime_voice/client.rs` 中：

1. 使用 `IntoClientRequest` 从 WebSocket URL 生成完整标准握手请求。
2. 将登录 `token` 作为敏感请求头插入已生成的请求。
3. 把完整请求交给现有 `connect_async`、超时和错误分类流程。
4. 连接成功后继续发送现有 auth 首帧，不改变 Fusion 实时语音 contract。

不手工生成 WebSocket 握手头，也不修改 Fusion 网关鉴权策略。这样可以复用依赖库的
随机 key 生成、Host 解析和协议版本处理，并保持网关安全边界不变。

## 错误与安全

- URL 或 `token` 无法转换为合法请求时，返回现有的配置错误，不发起网络连接。
- HTTP Upgrade 被网关拒绝时，继续使用现有连接错误分类。
- 日志只记录连接状态，不记录请求头或 token。
- auth 首帧和 Upgrade 请求头复用已有登录 token，不新增凭据存储。

## 验证

- 核对生成请求包含 `tokio-tungstenite 0.26` 要求的五个标准握手头和 `token`。
- 运行 Rust 格式检查和 `git diff --check`。
- 沿调用链静态检查登录 token 获取、连接超时、失败清理、auth 首帧和 session 激活。
- 按仓库约定不新增测试文件，也不默认运行 Cargo、Tauri 或桌面测试；安装版端到端验证
  需要包含该修复的新版本。
