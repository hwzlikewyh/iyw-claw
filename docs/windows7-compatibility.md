# Windows 7 SP1 x64 / x86 专用构建

## 已确认的范围

2026-10-09 用户确认实施 Win7 编译、固定 WebView2、前端兼容、环境与更新隔离。
沿用 Tauri 2、Next 16、Tailwind 4 和 Fusion 现有制品、环境绑定及更新流程。
Win7 分发目标使用 `windows7/x86_64` 和 `windows7/i686`；普通 Windows 继续使用 `windows/x86_64` 与 `windows/i686`。
不允许把普通 Windows 制品作为 Win7 制品的回退。

## 实施与验收

1. 接通主程序和 helper 的 Win7 目标、标准库构建与产物收集路径。
   验证：构建计划、运行库身份校验、Win7 目标编译。
2. 内置 Microsoft WebView2 固定运行时 109。
   验证：构建前检查运行库文件和版本；打包不使用 Evergreen bootstrapper。
3. 将前端编译目标包含 Chromium 109，保留现有 CSS 回退与框架。
   验证：正常前端构建、编译目标及不受保护的现代 CSS 检查。
4. 隔离 Win7 的环境、代理工具和应用更新。
   验证：Fusion 校验与选择回归、环境 helper 编译、更新请求链路静态审查。
5. 在 Windows 7 SP1 x64 和 32 位 x86 实机验收安装、窗口、终端、工具初始化和升级。
   未取得实机证据前，不标记正式支持或发布 Win7 制品。

## 发布前置条件

- 目标机为 Windows 7 SP1 x64 或 x86，并安装 WMF 5.1 / Windows PowerShell 5.1 及其 .NET 前置组件。
  现有安装事务使用 PowerShell 5 的 `::new()` 与 CIM；原装 PowerShell 2 不兼容。
  Win7 安装器在写入安装目录前检查 PowerShell 版本并给出明确提示，不自动安装系统组件。
- Rust `nightly-2026-04-15` 与 `rust-src`，MSVC 和 Windows SDK；该 Rust 目标无预编译标准库。
- 已解压的、Microsoft 签名的 WebView2 Runtime 109，架构必须与安装包一致（x64 或 x86）。
- 经 Win7 实机验证的 Node/npm、Git、uv、Chromix、agent-browser 制品。
  现有 Node 24 和 Chromix 152 的普通 Windows 包不能代替这些制品。
- Fusion 部署支持 `windows7` 的服务端代码，并登记对应制品及不可变环境绑定。
  本次源码任务不直接写数据库、上传制品、部署服务或发布版本。

## 当前进度

- 构建与分发源码已接通；手动候选工作流并行构建 x64 / x86，正式 release 增加两个 Win7 架构的可选矩阵，默认关闭。
- 最终前端生产构建及 TypeScript 通过；全部 484 个生产 JS 文件在 Chromium 109.0.5414.46
  解析通过，登录页实际加载无脚本错误。PDF 文档加载、canvas 渲染、文字提取和经
  浏览器目标转译的 viewer 创建通过。这不能代替桌面 IPC、会话与终端真机验收。
- Win7 x64 主程序完成实际链接；已知不可用 DLL / 系统函数导入检查通过，动态 CRT 导入为空。
  i686 主程序 `cargo check --target i686-win7-windows-msvc --features tauri-runtime -Z build-std=std,panic_abort`
  与实际 debug 链接通过；本机 AWS-LC 使用 `AWS_LC_SYS_NO_ASM=1` 和 VS 自带 CMake，CI 按正常流程安装 NASM。
  `0.1.262` 主程序 443381248 字节，PE32/i686、静态 CRT 与 Win7 导入检查通过。
  内置电脑执行器目标、版本、源码指纹和星河只读身份入口在构建机运行通过。
  构建机上 `--internal-xinghe-runtime-info` 只读入口运行通过；不等于 Win7 启动验收。
  本地缓存隔离补全后的最终主程序已复验通过，debug 可执行文件大小 538294784 字节。
- 环境 helper 已按两个 Win7 目标生成可执行文件，并验证静态 CRT 及已知缺失 DLL。
  i686 helper 实际链接通过，在构建机运行 `--version` 和 `--identity` 通过，target 为
  `i686-win7-windows-msvc`、distributionTarget 为 `windows7`；不等于 Win7 实机验收。
- computer helper 已链接，版本 `0.1.260`、目标身份及源码指纹校验通过，
  两个 staged helper 的完整校验均通过；辅助程序的早期只读入口在构建机运行通过。
- Fusion 环境/更新隔离回归、相关 Go vet 与全量 build 通过。
- WebView2 Runtime 109.0.1518.78 x64 已取得并解压；入口微软签名状态为 Valid，
  两架构的 Microsoft 签名、PE machine、版本、关键文件及实际配置生成均已在本机通过。
- 构建器把固定运行时暂存到 `resources/webview2-win7/<版本>`，应用上下文使用相对路径；
  不把构建机绝对路径带入安装后的运行时。
- WinRT/MTA/capability 与 AppContainer SID 静态入口已改为动态解析，缺失时返回失败。
- NSIS 初始化嵌入对应 Win7 环境 helper；PowerShell 5.1 检查经实际 NSIS 编译验证，
  构建机版本放行、模拟 PowerShell 2 拒绝均通过。
- 已生成本地 debug 候选安装器（不代表生产签名）：
  `原助理_0.1.260_x64-setup.exe`，大小 `398919296` 字节，SHA-256
  `3c49084cd63e919b930a6463f24d89620bc96e5daf926a6ef02d83a7f5f40fb9`。
  产物位于隔离构建目录 `.codex-temp/win7-candidate/.../bundle/nsis/`，未上传或发布。
- i686 完成实际 NSIS 打包，生成 `原助理_0.1.262_x86-setup.exe`，316249211 字节，
  SHA-256 为 `c89318b15491d5f0edf099e067d77c5a0a6cce0405528d51c22cfa9228edabe2`。
  本地先以默认桌面配置执行 `cargo build`，再使用 Win7 overlay 做 bundle-only 内容验证；
  打包脚本核验 x86 helper、x86 固定运行时及空 bootstrapper 输入通过。
  此包仅为内容验证产物，不是 Win7 启动候选。正式 release 在编译时也传入 Win7 overlay，
  仍须完成正式签名流程、安装后提取核验与实机验收；本轮未执行本机安装器。
- Fusion 环境绑定准备接口支持单独的 Win7 x64 / x86 或两架构矩阵，保留普通发布矩阵规则；
  创建、复用、缺失组件不写入及非法矩阵拒绝的回归通过。
  两架构制品在事务前全部检查，任一缺失时两个绑定均不写入。
- Win7 实机与正式制品发布：待取得兼容组件和实机证据。

## 构建入口

```powershell
rustup toolchain install nightly-2026-04-15 --profile minimal --component rust-src
$env:IYW_WIN7_WEBVIEW2_FIXED_RUNTIME_PATH = 'D:\runtimes\Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x64'
pnpm tauri:build:win7 --no-sign
```

默认目标为 x64；设置 `TAURI_TARGET_TRIPLE=i686-win7-windows-msvc` 可构建 x86。安装器在对应 target 的
`release/bundle/nsis/`，
品牌文件名包含 `win7`。构建器先核验对应 x64 / x86 PE 架构、109 版本、Microsoft 签名和关键运行库文件；
缺失时停止。`.github/workflows/build-windows7.yml` 接收已归档 CAB 的 HTTPS URL
和 SHA-256，两个矩阵 leg 分别校验后并行构建候选包；不会运行 Win7 实机验收或创建正式发布。

```powershell
$env:TAURI_TARGET_TRIPLE = 'i686-win7-windows-msvc'
$env:IYW_WIN7_WEBVIEW2_FIXED_RUNTIME_PATH = 'D:\runtimes\Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x86'
pnpm tauri:build:win7 --no-sign
```

Fusion 已部署本轮代码且 Win7 组件已登记后，由发布人员使用现有权限执行：

```powershell
node src-tauri/scripts/prepare-environment-binding.mjs <新版本> --win7
node src-tauri/scripts/verify-environment-plan.mjs <新版本> --win7
```

前一条命令写入不可变环境绑定，只能在发布准备时明确执行；本轮未执行。
应用更新上传分别使用 `runtime=desktop,target=windows7,arch=x86_64|i686,packageKind=nsis`。
`release.yml` 的 `build-windows7` 矩阵同时构建两个 Win7 target，`max-parallel: 2`。
实机验收、兼容制品与绑定准备完成后，设置仓库变量 `IYW_WIN7_RELEASE_ENABLED=true` 启用。
release 准备阶段会额外准备并核验两架构绑定；开启后任一 Win7 构建失败，或任一安装器/签名资产缺失，均阻止发布。
复用 `release-windows-staged.yml`，主程序编译和最终打包都注入 fixed Runtime 109 配置。
签名输入包含已核验的运行时目录，通过完整 manifest 摘要检查后恢复到签名机器。
硬件 token 签名使用同一 concurrency group 与 `queue: max` 串行排队，编译仍并行。
`queue: max` 已按当前 GitHub 官方文档核对；本机 actionlint 的 schema 尚不认识该键，
静态检查只忽略该键及既有自托管 runner 标签，其余 workflow 检查通过。

需要配置四个仓库 secrets：

- `WIN7_WEBVIEW2_X64_CAB_URL`、`WIN7_WEBVIEW2_X64_CAB_SHA256`
- `WIN7_WEBVIEW2_X86_CAB_URL`、`WIN7_WEBVIEW2_X86_CAB_SHA256`

缺少对应架构的 secret 时立即失败，x86 不回退 x64 输入。
GitHub Release 安装器分别为 `iyw-claw_<版本>_win7-x64-setup.exe` 与
`iyw-claw_<版本>_win7-x86-setup.exe`，updater 平台键为
`windows7-x86_64` 与 `windows7-i686`。

## 已解决的编译缺口

- indexmap 1 的自动 std 探测不适用于无预编译标准库的目标，Win7 显式启用已有 `std` feature。
- windows-link 0.1.3/0.2.1 在 Win7 的经典 COM 函数重定向到 `ole32.dll`，其余平台沿用上游绑定；
  WinRT activation、MTA usage 和 capability SID 的可选入口动态解析，缺失返回失败，
  未为 WinRT 伪造实现；补丁保留上游许可证。
- 固定 nightly 避免较新 nightly 的 Infallible/never 类型变化与现有 allocative 冲突。
- PDF 文档、viewer 和 worker 均切换同版本的官方 legacy 入口，CSS 增加旧浏览器交互颜色回退；
  页面和 worker 补齐 `Promise.withResolvers`、ReadableStream 异步迭代；viewer 的 Unicode sets
  正则经 Next 现有转译流程处理。
- Win7 禁用系统 WinRT Toast 插件并返回明确的不可用错误，保留应用内通知；
  `windows_slim_errors` 避免错误构造路径静态引用 WinRT 错误库，错误码仍可记录。
- Win7 包身份查询返回缺失，注册包 runner 继续要求真实 OS 包身份；未降低授权要求。
- Win7 Agent 本地缓存使用 `windows7-x86_64` / `windows7-i686`，工具目录使用 `win7-x64` / `win7-x86`，
  旧 `win-x64` 布局不导入为 Win7 库存，离线回退仍须匹配当前平台。
- Win7 禁止普通 Windows 的 Node/Git/uv 固定版本回退下载，缺少专用托管制品时保留失败。
- x86 可选 Windows 系统入口使用未修饰导入名，避免 `_GetProcAddress@8` 等导致加载器入口点缺失；
  导入门禁同时拒绝这些已发现的错误符号。
- 打包前检查目标身份、CRT、已知不可用 DLL 及导入函数；普通 Windows helper 已被该检查拒绝。
- windows-targets 0.48 的导入库构建脚本不识别 Win7，局部补丁仅为 Win7 启用它已有的 raw-dylib 宏分支。

Win7 本身没有 AppContainer 与 WinRT。相关运行请求必须报告不支持，不能把
沙箱权限错误转换为无沙箱执行，也不能把 registered package identity 缺失视为授权。

## Win7 实机验收记录要求

验收记录包含 OS 版本、SP1、x64 或 x86、PowerShell 版本、安装包版本与 SHA-256，
并保留安装初始化日志。使用已有安装事务执行，不绕过 Fusion 环境绑定或组件检查。

1. 从未安装 WebView2 的 Win7 安装，固定 Runtime 109 随包落入 `app` 目录，
   不启动 `MicrosoftEdgeUpdate.exe`；旧版升级使用同样路径。
2. 登录、聊天、文件预览（含 PDF）和桌面 IPC 可用，主窗口关闭、恢复与退出正常。
3. cmd / PowerShell 管道终端有输入输出，子进程停止与退出正常；Win7 无 ConPTY 尺寸调整。
4. 环境初始化选择全部必需组件时，只下载当前 `windows7/x86_64` 或 `windows7/i686` 的确切制品。
   缺少组件或绑定应明确失败，重试及事务回滚保留用户 data/config/runtime。
5. 星河会话与工具核心流程可运行；请求 AppContainer/WinRT 能力必须失败，
   不降低沙箱或注册包授权要求。
6. Win7 应用更新只收到 Win7 安装器；更新前后核对环境和用户数据摘要。
   全部通过后才登记正式支持与发布渠道。

本轮没有 Win7 实机证据、兼容组件制品或正式绑定，所以尚不能声称安装与全部功能通过。
已通过的 i686 debug 链接、只读入口和 NSIS 内容验证不能替代生产 profile、签名流水线与实机检查。

32 位版本沿用项目既有的能力边界：SQLite 记忆数据保留，Qdrant Edge 语义索引只在
64 位构建启用；没有已验证的 Win7 x86 cua-driver，电脑操作驱动安装明确拒绝。

## WebView2 归档记录

归档来源为 https://github.com/westinyang/WebView2RuntimeArchive/releases/tag/109.0.1518.78 。
微软官网目前不提供这一旧版固定运行时，所以归档来源不是官方发布站；使用前按微软
Authenticode 签名和文件版本核验，未绕过签名检查。
文件 `Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x64.cab`，大小
207090243 字节，下载后 SHA-256 为
`7622281cf83de1a35e3a471f432f7a897d65f0a7d3975df08512b7b253dd45c7`。
入口签名主体为 `Microsoft Corporation`，版本 `109.0.1518.78`。

x86 归档 `Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x86.cab` 大小
186447701 字节，下载后 SHA-256 为
`c507e0df03fe941f6669b74faf713545708653d568d1dce1e683cbe707382253`。
本机完成解压及 Microsoft 签名、109.0.1518.78 版本和 x86 PE 架构检查。

## 依据

- Rust target 指南：https://doc.rust-lang.org/nightly/rustc/platform-support/win7-windows-msvc.html
- WebView2 分发：https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- 当前生产 CSS 已生成 RGB 和 `@supports` 回退；不能仅凭出现 `color-mix()` 判定整个界面无法运行。
