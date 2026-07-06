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

单独准备 MCP sidecar：

```bash
pnpm tauri:prepare-sidecars
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
cargo check --no-default-features --bin iyw-claw-server
cargo test --no-default-features --bin iyw-claw-server --lib
cargo clippy --no-default-features --bin iyw-claw-server --lib -- -D warnings
```

MCP sidecar 检查：

```bash
cd src-tauri
cargo check --no-default-features --bin iyw-claw-mcp
cargo clippy --no-default-features --bin iyw-claw-mcp -- -D warnings
```

## 服务端配置

服务端支持通过环境变量配置：

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `IYW_CLAW_PORT` | `3080` | HTTP 端口 |
| `IYW_CLAW_HOST` | `0.0.0.0` | 监听地址 |
| `IYW_CLAW_TOKEN` | 随机生成 | Web 访问 Token |
| `IYW_CLAW_DATA_DIR` | 系统默认数据目录 | 数据库和上传文件目录 |
| `IYW_CLAW_STATIC_DIR` | `./web` 或 `./out` | 前端静态资源目录 |
| `IYW_CLAW_MCP_BIN` | 未设置 | `iyw-claw-mcp` 可执行文件路径 |
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
