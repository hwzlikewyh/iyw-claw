# Windows 普通用户安装与失败恢复

## 目标与验收

2026-09-28：安装前 `GetFullPath` 报非法路径，Chromix 解压
`chrome_elf.dll` 时在第 3 次尝试后报 Windows 错误 5。
目标是在普通用户权限下完整安装环境，短暂故障先自动恢复。

- 安装器不主动申请管理员权限；应用默认写入当前用户 LocalAppData，
  环境写入操作系统用户目录 `.iyw-claw`。
- 兼容安装目录外围的一对引号，保留中文和空格；拒绝内部非法字符与相对路径。
- 目录预检及文件操作遇到 Windows 错误 5、32、33 时退避重试。
- 单个组件准备失败只重试该组件，保留当前事务内其他已准备组件和校验过的缓存。
- 必需组件未就绪不能报告成功；最终失败提供原因，保留原有回滚和人工重试入口。

## 一手资料与取舍

| 项目或资料 | 已核对的做法 | 本项目采用方式 |
| --- | --- | --- |
| [uv 文件操作](https://github.com/astral-sh/uv/blob/1e052e3bc35e93db727294b13fae84dc42aa7189/crates/uv-fs/src/lib.rs) | 临时文件写入、原子就位，Windows 文件移动采用指数退避；该版本等待总量约 10 秒 | 解压文件在同目录临时写入并关闭，再改名就位；文件操作等待最多约 15 秒 |
| [Playwright 浏览器安装](https://github.com/microsoft/playwright/blob/b9a34ac7783a1b6c2e1dfff0c08ac744c048fa59/packages/playwright-core/src/server/registry/browserFetcher.ts) | 独立临时目录，浏览器下载安装失败清理后重试，最多 5 轮 | 复用已有事务隔离；可恢复文件错误最多准备 3 轮，重试前清理当前组件暂存内容 |
| [Electron Builder 解压流程](https://github.com/electron-userland/electron-builder/blob/4bc95cce39b2862e798365dc4687443e64f001a1/packages/app-builder-lib/templates/nsis/include/extractAppPackage.nsh) | 文件复制失败先自动重试，每次等待 1 秒，再提供交互处理 | 先自动恢复，再显示重试入口；不采用其最后忽略解压错误的兜底，必需组件仍须验证完整 |
| [VS Code 安装引导](https://github.com/microsoft/vscode/blob/e6aaa2ac860f6f38d901cbef97f2b9fc1321fa99/build/win32/code.iss) | 用户安装使用 `PrivilegesRequired=lowest` 和用户目录；使用安装互斥锁 | NSIS 使用 `RequestExecutionLevel user`，移除主动 UAC 重启，保留已有互斥和进程识别 |
| [Rustup 下载](https://github.com/rust-lang/rustup/blob/b32adec5ab613c26d7f35e398c8a54f4a46f585b/src/dist/download.rs)、[安装事务](https://github.com/rust-lang/rustup/blob/b32adec5ab613c26d7f35e398c8a54f4a46f585b/src/dist/component/transaction.rs) | 摘要缓存、断点续传、旧文件备份与事务回滚 | 保留已有摘要缓存和回滚；本次没有新增 HTTP 断点续传 |
| [Tauri Windows 安装器](https://v2.tauri.app/distribute/windows-installer/)、[微软 WebView2 分发](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution) | 均支持当前用户安装；WebView2 从非提权进程启动时支持按用户安装 | 延用当前 Tauri 和 WebView2 bootstrapper，不改框架或依赖 |

微软 [CreateFileW 文档](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
明确列出了待删除文件、共享方式及文件属性相关的访问拒绝情况。
[uv 的维护者说明](https://github.com/astral-sh/uv/issues/20792)也记录了扫描程序
引起的临时占用。错误 5 本身不足以证明故障机存在杀毒软件拦截。

## 流程和重试边界

1. 检查目录 -> 校验路径、创建、写入、改名、删除和可用空间。
2. 下载组件 -> 校验 SHA-256；已有正确缓存直接复用。
3. 准备组件 -> 每个文件先完整写入独立 `.part`，关闭句柄后在同目录改名。
4. 提交环境 -> 延用文件验证、旧环境备份和失败回滚。
5. 提交应用 -> 全部成功后才显示完成。

文件操作从 100 毫秒开始退避，单次等待上限 2 秒，总等待预算约 15 秒。
预检查使用相同等待策略。正常成功路径没有额外等待。
文件预算不包含操作本身的阻塞耗时，整个安装仍受现有 worker 阶段超时约束。

组件准备最多尝试 3 轮，间隔 1 秒、3 秒。只识别错误链中的 Windows
错误 5、32、33；不会对已耗尽下载重试的 NETWORK/INTEGRITY 错误再套一层下载重试。
每轮重新打开归档，不能从已经消费一部分的解压流继续写，否则会丢失数据。
用户点击最终弹窗的“重试”仍会重新准备整个环境事务，复用下载缓存；
组件内自动重试则保留同一事务中此前准备完成的组件。

worker 以 UTF-8 读取 helper 输出，将本轮错误写入状态文件供 NSIS 展示。
错误弹窗分类显示网络、完整性或访问拒绝，日志保留完整错误链和每次重试。

## 路径问题的证据边界

在安装器使用的 Windows PowerShell 5.1 中，带外围引号的路径可产生与截图
一致的 `GetFullPath` 异常；中文与空格本身合法。
现在通过 NSIS 栈传递路径，避免 System 插件将路径内引号当成调用语法，
并将规范化路径回传 NSIS，使检查目录与实际安装目录一致。
尚未取得故障机安装包版本、原始路径及日志，不能确认截图一定由外围引号触发。

PowerShell 5.1 脚本保持 ASCII 源码，避免没有 BOM 的 UTF-8 中文注释被系统
ANSI 编码误读。用户路径和结果文件仍支持 Unicode。

## 验证记录

- `cargo check --locked --manifest-path src-tauri/environment-core/Cargo.toml` 通过。
- 本次修改的 Rust 文件 `rustfmt --check` 通过。
- 两个修改的 PowerShell 脚本通过 Windows PowerShell 5.1 语法解析。
- 只读调用路径规范化函数：中文、空格和外围引号可解析；内部引号、相对路径被拒绝。
- `installer-worker-native.cs` 在 PowerShell 5.1 / .NET Framework 下编译通过。
- NSIS 3.11 使用本机 Tauri 生成模板编译当前 hooks 通过，输出 manifest 为
  `<requestedExecutionLevel level="asInvoker" uiAccess="false"/>`。
  模板版本为 0.1.237，省略应用 payload，仅核对 hooks，不能当成 0.1.243 发布包。
  模板仍报告已有的 `UNINSTKEY`、`BUNDLEID`、`NoShortcutMode`、`PassiveMode`
  声明顺序及未使用标签告警。
- 按仓库 AGENTS.md 未新增测试文件或运行测试套件，未启动真实安装器。
- 本次改动的 `git diff --check` 通过；临时 NSIS 编译脚本及输出已清理。

没有实测故障机、受限账户、文件占用注入或安全软件拦截；没有统计安装成功率。
已存在的显式拒绝 ACL、持续拦截、磁盘不足仍可能使安装失败，重试不改变这些限制。
更改需要重新构建并签名安装包后交付，现有安装包不会随源码修改而更新。
