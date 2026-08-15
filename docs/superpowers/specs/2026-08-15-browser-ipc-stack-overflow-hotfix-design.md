# 浏览器 IPC 栈溢出热修设计

## 背景

`0.1.82` 升级后的首次启动曾在主窗口创建成功后异常退出。Windows 事件与转储确认：

- 异常码为 `0xc00000fd`，即 `EXCEPTION_STACK_OVERFLOW`。
- 崩溃线程是桌面主线程，调用链位于 WebView2 IPC 消息处理期间。
- 栈展开显示连续出现约 `226 KB`、`453 KB`、`226 KB` 的原生栈帧。
- 崩溃前浏览器 capability 已完成验证，浏览器 daemon 尚未启动。
- 同一安装包第二次启动成功，说明安装文件、数据库迁移和 sidecar 完整性不是主因。

Tauri `2.10.2` 在 release 模式中会将异步 command task 按具体类型传给异步运行时；debug 模式则会先执行 `Box::pin`。浏览器 command 引入的异步 future 体积较大，release 路径在 WebView2 主线程上搬运这些 future 时耗尽默认线程栈。

## 目标

1. 消除浏览器 Tauri command 在 release 模式下的大型栈帧。
2. 覆盖启动探测、浏览器运行时、页签、控制权、帧流和 CDP 操作，避免只修启动后在首次使用时复发。
3. 保持前端 command 名称、参数、返回值和错误 envelope 不变。
4. 不修改浏览器会话状态机、Agent 权限模型、profile 锁和 UI 布局。
5. 作为 `0.1.83` Windows 热修发布。

## 方案

### 统一 command future 类型

在 `src-tauri/src/commands/browser/mod.rs` 定义内部类型：

```rust
type BrowserCommandFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, BrowserError>> + Send + 'static>>;
```

该类型只服务于 Tauri IPC 边界，不进入浏览器核心模块或公共数据 contract。

### 转换浏览器异步命令

将 `src-tauri/src/commands/browser/` 下的异步 Tauri command 转换为：

- 使用 `#[tauri::command(async)]` 保持异步执行语义。
- command 函数本身返回 `BrowserCommandFuture<T>`。
- 从 `tauri::State` 克隆轻量的 `BrowserSessionManager` 句柄。
- 业务调用保留在 `Box::pin(async move { ... })` 内。

示意：

```rust
#[tauri::command(async)]
pub fn browser_refresh_capability(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    Box::pin(async move { Ok(manager.refresh_capability().await) })
}
```

装箱后，Tauri release invoke task 只在主线程栈上传递固定大小的智能指针，具体 command future 位于堆上。浏览器核心方法及其返回语义不变。

### 覆盖范围

覆盖以下 command 文件中的全部异步入口：

- `runtime.rs`
- `tabs.rs`
- `streams.rs`
- `views.rs`
- `control.rs`
- `cdp.rs`

同步窗口创建和关闭命令保持现状，因为它们不生成大型异步 future。

## 错误与兼容性

- `BrowserError` 原样返回，前端不需要适配。
- command 名称及 camelCase 参数序列化规则不变。
- `BrowserSessionManager` 的克隆只复制 `Arc` 等共享句柄，不复制浏览器状态。
- future 要求 `Send + 'static`；所有输入均为拥有所有权的数据，`tauri::State` 在进入 future 前转换为克隆后的 manager。
- 不修改 Tauri 依赖版本，避免热修同时引入框架升级风险。

## 验证

### 静态验证

- `cargo fmt --check` 覆盖 Rust 格式。
- `cargo check --release` 或等价 release 编译检查，确保装箱后的生命周期、`Send` 和 command 宏展开正确。
- 沿前端 `browserApi` 到 Rust command 检查名称、参数和返回类型未变化。
- `git diff --check` 检查补丁格式。

### Windows 运行验证

debug 模式使用不同的 Tauri 装箱路径，不能作为本缺陷的最终证据。必须验证 release 二进制：

1. 使用现有用户数据连续冷启动至少 10 次，确认无新增 `0xc00000fd` 事件或 crash dump。
2. 在启动阶段重复双击应用，确认单实例转发不会导致退出。
3. 打开内置浏览器，完成 capability 探测和 runtime 启动。
4. 执行新建页签、导航、输入、截图/帧流和关闭浏览器。
5. 通过 Agent 浏览器工具至少完成一次列出页签和页面快照。
6. 关闭并重新打开主窗口，确认托盘恢复和浏览器状态刷新正常。

## 发布条件

- 版本统一升级到 `0.1.83`。
- 完成仓库约定的静态审查和 release 构建。
- 生成 Windows 安装包与 updater artifact，并在独立路径完成安装验证。
- 发布 GitHub Release 后更新 Fusion artifact，验证普通下载和 Tauri updater 地址均可用。
- 若 release 运行仍出现栈溢出，不发布；保留转储并继续按实际栈帧定位。

## 非目标

- 不重构全局 `generate_handler!` 列表。
- 不修改 Tauri 上游 crate 或启用全局动态分发。
- 不改变浏览器引擎、sidecar 校验、profile 数据和 Agent 控制权协议。
- 不把本次一次性诊断工具或转储文件加入仓库。
