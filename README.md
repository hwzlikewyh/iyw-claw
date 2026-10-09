# iyw-claw

iyw-claw 是一个多智能体编码工作台，用于在同一个工作区内管理代码项目、会话、终端、文件、Git 操作和多个 AI 编码代理。项目支持桌面应用、独立服务端和 Docker 部署。

## 功能概览

- 聚合多个编码代理的会话与任务。
- 在同一工作区内查看文件、终端、Git 变更和对话。
- 支持多智能体协作与子任务委托。
- 支持本地桌面运行，也支持浏览器访问的服务端模式。
- 支持 SQLite 数据存储、WebSocket 实时事件和静态前端导出。
- 支持自动化任务、消息渠道、模型供应商配置和运行日志查看。

## 技术栈

- 桌面端：Tauri 2
- 后端：Rust、Axum、SeaORM、SQLite
- 前端：Next.js 16、React 19、TypeScript
- 样式：Tailwind CSS v4、shadcn/ui
- 包管理器：pnpm

## 环境要求

- Node.js 22 或更高版本
- pnpm 11 或更高版本
- Rust stable
- 桌面模式需要安装对应系统的 Tauri 构建依赖
- macOS 桌面端要求 13.5 或更高版本（与内置 Node.js 24 运行时一致），支持 Intel 和 Apple Silicon

### Windows 7 兼容性

Windows 7 SP1 x64 和 32 位 x86 使用单独的 Rust 目标
`x86_64-win7-windows-msvc` 与 `i686-win7-windows-msvc`，这些目标没有
Rust 官方预编译标准库，需要在构建机上按 Rust 的 Win7 target 指南构建标准库并配置
MSVC/Windows SDK。普通 Windows 构建目标仍为 `x86_64-pc-windows-msvc`。
安装目标机还需 Windows PowerShell 5.1（WMF 5.1 及其 .NET 前置组件），
因为现有安装事务使用 PowerShell 5 的构造语法和 CIM；Win7 自带 PowerShell 2 不满足要求。

Windows 7 没有 ConPTY。应用会使用管道终端运行 cmd、PowerShell 和外部命令，终端尺寸
调整不可用；Windows 10 1809 及更新版本继续使用 ConPTY。Microsoft Edge/WebView2
官方说明 Windows 7 支持到 WebView2 Runtime 109。当前 Evergreen 引导安装器可能调用
Win7 不支持的接口，Win7 构建不使用该安装路径，必须设置
`IYW_WIN7_WEBVIEW2_FIXED_RUNTIME_PATH`，指向已解压的 WebView2 Runtime 109 目录
（x64 使用 `Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x64`，x86 使用对应的
`.x86` 目录），构建器会自动切换到 fixed runtime，避免启动新版
`MicrosoftEdgeUpdate.exe`。使用
`pnpm tauri:build:win7 --no-sign` 构建候选包；需要 Rust
`nightly-2026-04-15` 与 `rust-src`。Win7 的 Fusion 环境、代理和更新目标为
`windows7/x86_64` 或 `windows7/i686`，缺少专用组件或绑定时明确失败，不回退普通 Windows 制品。
候选包尚须在真实 Win7 SP1 x64 和 32 位环境验证安装、界面、终端、环境组件和升级，
验证前不视为正式支持。准备及验证清单见 [Windows 7 专用构建](docs/windows7-compatibility.md)。

## 安装依赖

```bash
pnpm install
```

## 开发运行

仅运行前端开发服务：

```bash
pnpm dev
```

运行桌面应用开发模式：

```bash
pnpm tauri dev
```

运行独立服务端开发模式：

```bash
pnpm server:dev
```

## 构建

构建前端静态资源：

```bash
pnpm build
```

构建桌面应用：

```bash
pnpm tauri build
```

macOS 默认生成 `.app` 和 `.dmg`。构建应用与环境引导程序：

```bash
pnpm tauri:build:prod
```

该入口会准备并验证环境引导程序、构建前端和应用。Node/npm、Git、uv、Chromix、agent-browser 及后台选定的可选工具由安装引导从 Fusion 环境计划下载，不内置 Agent SDK 或记忆模型。
本地无签名环境使用 `pnpm tauri:build:fast` 需要已有 `out/`；完整重新构建可使用
`pnpm tauri:build:prod --no-sign`。在 Apple Silicon 上构建并验证 Intel 包需要 Rosetta 2，
通过 `TAURI_TARGET_TRIPLE=x86_64-apple-darwin` 选择 Intel 目标，并预先安装对应 Rust target。

构建独立服务端：

```bash
pnpm server:build
```

准备桌面应用捆绑的环境引导程序（`iyw-environment`）：

```bash
pnpm tauri:prepare-sidecars
```

环境安装完成后在用户目录生成独立修复入口；修复程序复用已校验组件，缺失或损坏时重新下载。GitHub Release 发布前校验当前版本的五个平台环境计划，缺少绑定或制品时阻止发布。

重复安装先读取当前版本的 Fusion 环境计划，再核对已安装组件的制品身份、目录和必要
入口文件；匹配的组件直接复用，不重新下载、解压或扫描全部文件，不计算已有组件摘要。
新装或更新组件仍完整校验，显式修复和诊断也保留全量检查。普通升级不会主动发现已有
组件的非入口文件损坏或同大小内容变更，发现运行异常时请执行环境修复。校验保持串行、
流式读取，不增加并行扫描或文件内容缓存。Agent Reach 中有对应受保护源码的 Python
字节码缓存允许正常生成或变化；完整检查仍保护源码与无源码字节码。

## Docker 运行

使用 Docker Compose：

```bash
docker compose up -d
```

直接使用 Docker：

```bash
docker build -t iyw-claw .
docker run -d -p 3080:3080 -v iyw-claw-data:/data iyw-claw
```

### HTTP-only 内置 MCP 迁移

内置 MCP 由 `iyw-claw-server` 或桌面主进程在 loopback 上提供
Streamable HTTP `/mcp`。当前版本不会构建、启动或发布独立的
`iyw-claw-mcp` 可执行文件。

从 `v0.1.92` 或更早版本运行服务端的用户，第一次迁移必须重新执行安装器（Linux）
或 `install.ps1`（Windows）；当前 server release workflow 未发布 macOS server 归档，
macOS 安装器会 fail-closed。Docker 部署必须更新源码并重新构建部署。旧版服务端的内置
updater 只认识包含 MCP companion 的归档，不能原地升级到首个 HTTP-only 归档，因此不要
在旧版本上等待内置更新完成。

以下命令中的 tag 必须已经发布且包含对应签名资产。若 `v0.1.93` 尚未发布，命令应失败；
不要改用 `main` 或 `latest` 绕过固定版本门禁。

```bash
# Linux：固定脚本与归档使用同一个 HTTP-only tag；目录必须替换为原 server/web 安装目录
http_only_tag=v0.1.93
curl -fsSL "https://raw.githubusercontent.com/hwzlikewyh/iyw-claw/${http_only_tag}/install.sh" \
  | bash -s -- --version "${http_only_tag}" --dir "${IYW_CLAW_INSTALL_DIR:-/usr/local/bin}"
```

```powershell
# Windows PowerShell：固定脚本与归档使用同一个 tag，并明确复用原 server 安装目录
$httpOnlyTag = "v0.1.93"
$script = irm "https://raw.githubusercontent.com/hwzlikewyh/iyw-claw/$httpOnlyTag/install.ps1"
& ([scriptblock]::Create($script)) -Version $httpOnlyTag -InstallDir "$env:LOCALAPPDATA\iyw-claw"
```

```bash
# Docker 源码部署：只在干净的部署 checkout 中切到已发布 HTTP-only tag 后重建容器
test -z "$(git status --porcelain)" || { echo "deployment checkout is not clean" >&2; exit 1; }
git fetch --tags origin
git checkout --detach v0.1.93
docker compose up -d --build --force-recreate
```

Linux 自定义 web 目录同时设置 `IYW_CLAW_WEB_DIR`；Windows `-InstallDir` 必须指向
原有 `iyw-claw-server.exe` 所在目录。安装器要求系统预装 `minisign`，会在停服或清理
旧 MCP 前验证固定 tag 的 archive 签名、内容清单、目标目录和 staged server 版本。
当前 latest 仍可能是旧版本时，必须显式传入 `v0.1.93` 或更新的已发布 HTTP-only tag。

迁移完成后，后续版本的服务端 self-update 才会使用固定版本 tag 下载
`server + web` 归档；安装器仅清理旧 MCP 文件和进程，不会重新安装或恢复 companion。

如果需要指定访问 Token：

```bash
docker build -t iyw-claw .
docker run -d -p 3080:3080 \
  -v iyw-claw-data:/data \
  -e IYW_CLAW_TOKEN=your-secret-token \
  iyw-claw
```

## 常用检查

前端 lint：

```bash
pnpm eslint .
```

前端测试：

```bash
pnpm test
```

覆盖率：

```bash
pnpm test:coverage
```

Rust 检查：

```bash
cd src-tauri
cargo check
cargo test --features test-utils
cargo clippy --all-targets --features test-utils -- -D warnings
```

服务端模式检查：

```bash
cd src-tauri
cargo check --no-default-features --features server-runtime --bin iyw-claw-server
cargo test --no-default-features --features server-runtime --bin iyw-claw-server --lib
cargo clippy --no-default-features --features server-runtime --bin iyw-claw-server --lib -- -D warnings
```

内置 MCP 由主进程通过 Streamable HTTP 提供，不需要额外构建或安装 MCP 可执行文件。

## 服务端配置

服务端支持通过环境变量配置：

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `IYW_CLAW_PORT` | `3080` | HTTP 端口 |
| `IYW_CLAW_HOST` | `0.0.0.0` | 监听地址 |
| `IYW_CLAW_TOKEN` | 随机生成 | Web 访问 Token |
| `IYW_CLAW_DATA_DIR` | 系统默认数据目录 | 数据库和上传文件目录 |
| `IYW_CLAW_STATIC_DIR` | `./web` 或 `./out` | 前端静态资源目录 |
| `IYW_CLAW_SKIP_SIDECAR` | 未设置 | 跳过 sidecar 构建 |

## 目录结构

```text
src/          前端应用代码
src-tauri/    Rust 后端、Tauri 应用和服务端代码
public/       前端静态资源
scripts/      项目脚本
```

## 许可证

Apache-2.0
