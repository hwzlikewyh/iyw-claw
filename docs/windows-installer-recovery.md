# Windows 普通用户安装与失败恢复

## 关键应用文件缺失提示（2026-10-09）

- 验收：新 app 的主程序或环境 helper 缺失时，显示文件路径、原始 Windows 错误码与检查次数，并提示“文件可能被安全软件隔离，请查看防护记录或隔离区”。错误码 2/3 只能说明文件或路径不存在，不能确定发生了杀毒拦截，更不能确定厂商。
- 提示只附加到新 app 校验失败，不改变历史备份的完整性判断。访问拒绝、共享冲突和零字节保留原有错误分类。
- 初次应用校验增加与最终提交一致的交互重试入口：确认文件来源可信并恢复文件后重试完整校验；取消则停止环境 worker，保留失败现场并按原事务恢复旧版本。无法恢复文件时提示重新运行安装包。
- 静默和 passive 安装继续直接进入原失败回滚流程，不弹出重试对话框。校验失败及最终恢复状态写入安装日志，不关闭安全软件或自动设置排除项。
- 源码修改需重新构建并签名安装包后生效；未提交厂商误报申诉。
- 验证：普通 WebView2 bootstrapper 与固定运行时两种模式的 NSIS hooks 均编译通过；静态复核覆盖重试成功、取消回滚、静默/passive 分支与原始错误保留。编译省略应用 payload 和快捷方式 COM 宏，未运行测试套件、真实安装或安全软件隔离复现，临时编译文件已清理。

## 安装阶段与等待反馈（2026-10-08）

- 验收：进入安装页能看到当前动作；初始化期间持续显示用时；连续 30 秒没有新进度时显示等待提示；失败带原因与日志；只有完整安装成功才显示 100%。
- WebView2 由 Tauri 的 `WebView2` Section 安装，执行顺序在 `NSIS_HOOK_PREINSTALL` 和环境 worker 之前。原有自定义百分比直到 worker 启动才更新，因此 WebView2 耗时或失败时会停在 0%。
- 保留 Tauri `embedBootstrapper` 与当前用户安装方式。安装前用滚动活动条和“准备中”表示尚无可量化进度；WebView2 缺失时显示专用安装提示。固定运行时构建不展示 Evergreen 安装提示。
- 应用准备阶段显示目录检查、进程清理、旧版本备份和任务启动。worker 根据现有进度文件与应用写入进度显示当前动作和用时；文件更新时间、阶段或数值变化会清除等待提示。
- 30 秒无进度只提示等待，不提前判定失败。阶段变化和进入等待状态各记录一次；20 分钟阶段超时包含停留动作和耗时。完成、失败停止活动条，当前步骤仅在后续步骤开始后标记完成。
- WebView2 安装失败独立报告 bootstrapper 返回码，写入 `%TEMP%\iyw-claw-webview2-install.log`，提供微软官方 Evergreen Standalone Installer 入口及 `msedge_installer.log`、`MicrosoftEdgeUpdate.log` 的常见位置。应用替换事务此时尚未开始。

### `0x8004070c` 的诊断边界

截图只能确认微软安装器返回该错误，不能单凭错误码认定权限冲突、注册表残留或 VC++ 运行库缺失。
微软 [WebView2 分发文档](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)明确支持普通用户和机器级安装，并说明少数 Windows 10 设备可能没有运行库。
官方反馈仓库 [#5491](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5491)记录 Windows 10 22H2 下管理员运行离线安装器仍出现相同错误；[#1863](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1863)中微软要求收集 Edge 安装日志。
因此优先尝试与系统架构匹配的官方离线安装器，复现后读取失败机器同一时间的日志；不自动删除 Microsoft 目录、注册表或关闭安全软件。

本次按仓库规则不新增或运行测试套件。已完成 PowerShell 5.1 语法解析、.NET Framework C# 编译、普通及固定运行时 NSIS hooks 编译和无系统写入的安装页预览。编译检查省略应用 payload 与快捷方式 COM 宏，不能替代完整发布包验证。未复现故障机器的 WebView2 错误，未生成签名发布包；源码改动需重新打包后生效。

## 目标与验收

2026-09-28：安装前 `GetFullPath` 报非法路径，Chromix 解压
`chrome_elf.dll` 时在第 3 次尝试后报 Windows 错误 5。
目标是在普通用户权限下完整安装环境，短暂故障先自动恢复。

- 安装器不主动申请管理员权限；应用默认写入当前用户 LocalAppData，
  环境写入操作系统用户目录 `.iyw-claw`。
- 兼容安装目录外围的一对引号，保留中文和空格；拒绝内部非法字符与相对路径。
- 目录预检及文件操作遇到 Windows 错误 5、32、33 时退避重试。
- 单个组件准备失败只重试该组件，保留当前事务内其他已准备组件和校验过的缓存。
- 备份旧 app 前通过 Windows Restart Manager 按实际 `exe/dll/node/ocx` 文件查询占用进程；
  只停止当前用户且身份校验通过的安装实例进程。
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

### app 目录占用

Windows 错误 32 (`ERROR_SHARING_VIOLATION`) 说明目录内至少一个文件仍被打开。
仅按进程名或可执行文件目录筛选无法覆盖 WebView2、插件宿主和从 app 目录加载 DLL
但自身位于系统目录的进程。安装器现在把 app 下的二进制文件注册到 Windows
[Restart Manager](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmregisterresources)，
用 `RmGetList` 获取实际 PID，再复用已有的 owner、启动时间、路径和脚本身份校验。
只有同一当前用户且被确认属于本安装实例的 PID 才会停止；系统进程、其他用户进程、
身份变化的 PID 和无法确认归属的进程继续阻断并显示重试。

第一次改名失败后会重新扫描并停止一轮，覆盖进程在安装开始后才加载 DLL 的竞态；
进程等待预算由 5 秒增加到约 10 秒。关闭资源管理器目录、终端、文件预览和第三方
工具仍是必要的人工兜底，持续被安全软件拦截时不会强行删除或覆盖文件。

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
- `installer-restart-manager.cs` 在 PowerShell 5.1 / .NET Framework 下编译通过，
  对未被占用的文件探测返回 0 个进程；保持文件句柄打开的模拟场景能返回当前
  PowerShell 进程 PID。
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

## 0.1.245 复现

- 下载包 `iyw-claw_0.1.245_x64-setup.exe` 的 Authenticode 签名有效，SHA-256 为
  `AAF4BB0592ED3C5BAD4C31F7B4DFDB2D1A1AE3D9F9BC05B5F8C4180993D8E7BB`。
- GitHub `v0.1.245` 指向提交 `6eabde46`，该提交只将版本号从 `0.1.244` 改为
  `0.1.245`，安装器代码来自此前主分支。
- 在隔离的 NSIS 测试根目录复现：新 app 顶层只有 `iyw-claw.exe`、
  `iyw-environment.exe`、许可证和卸载程序，没有 `iyw-claw-mcp*` 文件；安装器仍报告
  “新 app 仍包含旧 iyw-claw-mcp 文件”，随后恢复旧 app。
- 构建期清理和验证本身已经存在，误报发生在安装器提交校验：旧 MCP 检查通过临时
  PowerShell 子进程返回 `0/1/2`，该制品中出现了假阳性。
- 安装器提交校验已改为 NSIS 原生 `FindFirst/FindNext`，只匹配 app 或 backup
  顶层真正存在的 `iyw-claw-mcp*` 文件，并把文件路径写入错误信息；不再依赖该
  PowerShell 检查的退出码。

## 0.1.247 实际安装验收

- 对下载的签名包执行本机静默安装和可见更新：静默安装约 105 秒退出 0，
  可见更新约 30 秒完成；环境诊断返回 `healthy`，8 个受管组件均有清单记录，
  主应用窗口能启动并响应。
- 干净的 GitHub Windows runner 使用同一签名包完成首次安装、离线重装回滚、
  在线重装与用户数据保留；没有在验收中重新构建应用。
- 首轮验收的卸载器退出 `4294967295`：从 `app` 内执行的卸载器被本项目的
  进程清理逻辑识别为当前安装实例进程。NSIS 官方文档确认 `_?=` 会禁用卸载器
  自动复制到临时目录；验收现在显式复制到临时目录并等待执行完成。
- 清理逻辑现在传递安装器 PID，并排除调用方自身。独立 NSIS 探针验证：
  单独运行时 `check=0`，同目录另一个进程仍运行时 `check=1`。
- 本机曾发现生产安装根目录的注册表值误指向旧 smoke 目录；安装器初始化时
  已增加恢复逻辑。该源码修复和进程清理修复均不在已下载的 0.1.247 制品内。
- 本机终端为管理员高完整性令牌；目前不能据此声称普通用户完整安装已验证。
- 第二轮干净机器验收确认卸载成功，但找不到 `.iyw-claw/maintenance`：
  环境修复工具的部署模块在旧版重构后未再被引用。现已恢复在环境提交前
  安装独立修复工具；安装成功不应留下没有独立修复入口的环境。

## 0.1.247 staging 重打测试包

- 使用 `iyw-staging-36458055855-1-x86_64-pc-windows-msvc.zip` 中的已编译主程序、
  前端和资源，只重新编译环境 helper、运行 Tauri `bundle` 生成 NSIS；主程序未重新编译。
- 测试包在 `D:\Users\iyw\Downloads\iyw-claw_0.1.247_x64-setup-test-8074b87b.exe`，
  SHA-256 为 `02CCEAAA5F5874F4A08BBDDA25C5CB159BFE0B58B4638C3D6122D12F5D1FB599`。
  该包未签名，仅供安装验证，不作为正式发布件。
- 本机独立 smoke 根目录实际安装退出 0，约 5.7 秒，主程序和环境 helper 均落盘；
  测试卸载后临时安装目录及对应注册表项均不存在。smoke 模式跳过联网环境初始化，
  因此新包的完整生产安装还需单独验证。
- 环境 helper 静态 MSVC 运行库检查通过；机器上旧 0.1.239 安装进程持有互斥锁，
  首次测试返回 1618，清理两个已确认的旧安装进程后重试成功。

## 启动 UAC 排查

- 截图中的 UAC 不是安装器请求，而是 staging 内的 `iyw-claw.exe` 入口无条件调用
  Codex Windows sandbox 的 `ShellExecuteEx("runas")`。即使 NSIS 使用
  `RequestExecutionLevel user`，启动已安装应用仍会再次提权。
- 已移除桌面启动时的无条件提权；sandbox setup 仍保留按需提权路径，只在确实需要
  创建或修复 Windows sandbox 账户时请求权限。需要重新编译主程序，旧 staging
  以及刚生成的测试包不会自动获得这个修复。
- 实测旧 exe 的嵌入 manifest 只有 Common Controls，未包含链接参数所声明的
  UAC level。改用 Tauri `WindowsAttributes.app_manifest` 官方入口嵌入
  `requestedExecutionLevel="asInvoker"`，保留原 Common Controls 依赖。
- 使用项目 Tauri 发布配置编译新的 Windows x64 主程序，旧 staging 仅用于复用前端和
  其他资源；新 exe SHA-256 为
  `9C21DD2803DF494C1160B16177C9303421828770EBC4D7CEBC693F00F932D469`。
  生成安装包后，已安装 exe 的嵌入 manifest 确认 `asInvoker`、`uiAccess=false`。
- 新包 `D:\Users\iyw\Downloads\iyw-claw_0.1.247_x64-setup-no-uac-20260929.exe`
  SHA-256 为 `C2072DD624AE81029993553784015F8E7D4FFED4F82310ABE90A52D609A4ECA8`，
  未签名，仅供安装验证。隔离 NSIS 安装退出 0（约 5 秒），主程序及 helper 均落盘。
- `runas /trustlevel:0x20000` 启动隔离实例：启动器确认非管理员，主程序持续运行，
  检查时无 `consent.exe`。测试进程已停止，隔离目录和对应测试注册表已清理。
  smoke 模式跳过联网环境初始化，完整普通用户环境安装仍未由此验证。
