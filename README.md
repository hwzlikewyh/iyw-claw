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

构建独立服务端：

```bash
pnpm server:build
```

准备桌面应用捆绑的 sidecars（当前为受支持 Windows 平台的 `agent-browser`）：

```bash
pnpm tauri:prepare-sidecars
```

仅准备 `uv` / `uvx` Python 工具运行时：

```bash
pnpm tauri:prepare-sidecars --uv-only
```

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
