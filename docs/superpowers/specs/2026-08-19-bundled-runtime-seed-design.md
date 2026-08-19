# 全平台内置基础运行时种子设计

## 状态

已获用户确认，进入实现前设计固化阶段。

## 目标

让安装包在首次启动时可以离线提供基础运行环境：

- Node.js
- uv/uvx
- Git
- Codex ACP 运行时（`@agentclientprotocol/codex-acp`、平台对应的 `@openai/codex` 及其 npm 依赖）

适用目标：

- Windows x64
- macOS x64
- macOS arm64
- Linux x64
- Linux arm64

Windows x86 保留当前 Version Center 联网安装逻辑，不放置 Codex 种子，也不改变现有安装包发布入口。

内置种子只是离线基线，不覆盖用户已经激活的更高版本。种子损坏、缺失、架构不匹配或启动失败时，必须回退到现有 Fusion Version Center 的 resolve、票据下载、哈希/签名校验、原子激活和回滚流程。

## 非目标

- 不把 API token、登录信息、用户配置、会话、Skills 或 MCP 配置放进安装包。
- 不把所有平台的运行时放入每一个安装包；每个安装包只携带自身 target/arch 的种子。
- 不移除 Version Center，也不把联网更新改成永久不可用。
- 不为 Windows x86 伪造或移植当前 Codex 二进制。

## 现状与约束

当前 Claw 已通过 Version Center 管理 `node`、`uv`、`git`，并通过 npm offline archive 管理 `codex-acp`。安装器目前只包含应用资源和自身 MCP sidecar；`prepare-sidecars.mjs` 明确不再把 Node、Git、uv/uvx、Codex 放入安装器。Version Center 已有库存、激活、校验和回滚能力，内置种子应复用该生命周期，而不是创建第二套安装器状态。

跨平台 Git 不使用构建机系统 Git：系统 Git 可能依赖用户安装的 Xcode Command Line Tools、动态库或发行版包管理器。实现采用经过固定版本和 SHA-256 校验的可重定位发行包；优先使用 GitHub Desktop `dugite-native` 对 macOS/Linux 的发行包，Windows 继续使用现有 MinGit/Version Center 兼容布局。许可证、来源和校验摘要写入种子清单及第三方声明。

## 方案选择

### 方案 A：应用资源中的只读种子，启动时导入受管 runtime（采用）

CI 为每个目标下载固定版本的官方运行时包，验证来源摘要后生成一个 target/arch 专属种子包和清单。Tauri 只把该种子作为 resource 放入安装包。首次启动由桌面 bootstrap 发现种子，解包到 staging，按清单校验后调用现有 Version Center 激活逻辑；激活成功后种子保持只读或被标记为已消费。

优点：复用现有库存、原子激活、回滚和版本选择；应用更新只替换 app 区，不破坏 runtime 区；失败时可以逐组件回退联网安装。代价是需要增加种子导入适配层和 CI 资源准备步骤。

### 方案 B：直接把所有可执行文件作为 Tauri externalBin

Node、Git、uv 和 Codex 的多文件目录直接登记为 sidecar。该方式不适合 npm 目录、Git 运行库和 uv 缓存，无法自然复用现有激活/回滚，也容易被 Tauri/NSIS 当作应用文件替换。

### 方案 C：安装器自定义脚本解压到 runtime

NSIS、DMG 和 Linux 包分别实现解压、校验和迁移。跨平台脚本重复度高，错误恢复路径与当前 Version Center 分裂，且会扩大安装器故障面。仅保留方案 A 所需的最小安装资源声明，不在各平台安装器脚本中实现业务逻辑。

## 架构

### 种子布局

每个安装包只包含一个资源目录：

```text
runtime-seed/
  manifest.json
  components/
    node/<version>/<platform>/...
    git/<version>/<platform>/...
    uv/<version>/<platform>/uv
    codex-acp/<version>/<platform>/npm-prefix/...
  licenses/
    node.txt
    git.txt
    uv.txt
    codex-acp.txt
```

Windows x86 构建不生成 `runtime-seed`，保持当前逻辑。

`manifest.json` 至少包含 schema 版本、Claw 版本、target、arch、每个组件的版本、来源、包类型、相对入口、文件列表、总大小和 SHA-256。所有路径必须是相对路径，禁止 `..`、绝对路径和符号链接越界。

### 版本与覆盖规则

1. 启动时读取本地受管库存。
2. 已有有效 active 版本满足或高于种子版本时跳过种子导入。
3. 没有有效 active 版本，或 active 版本低于种子版本时，按组件导入种子。
4. 每个组件独立校验、激活和记录；Node、uv、Git 任一失败不阻塞其它组件，但 Codex 激活必须同时满足 Node 版本要求和 npm prefix 完整性。
5. 版本中心远端 offer 始终可以提供更高版本；远端激活成功后不被旧种子降级。

### 启动与回退

种子导入采用 `staging -> verify -> atomic activate -> inventory commit`，沿用现有 runtime installer 的临时目录、文件同步、回滚和 trash 机制。导入只在进程启动阶段或显式 runtime bootstrap 操作中执行，不在请求热路径执行。

启动探测顺序：

1. 受管 active runtime。
2. 安装包种子导入后的受管 runtime。
3. 用户系统 PATH（保留现有兼容行为）。
4. Fusion Version Center 在线安装。

种子验证或启动失败必须记录组件、版本、target/arch、失败阶段和错误原因，但不得记录 token、签名正文或完整用户路径；随后调用现有在线流程。在线流程失败时保留最后一个有效 active 版本，不删除可用旧版本。

### Codex 运行时

Codex 种子不是单个 `codex.exe`。CI 必须为目标平台生成完整 npm prefix，包含：

- `@agentclientprotocol/codex-acp`
- `@openai/codex` 的目标平台包
- 所有锁定的 npm 依赖和入口 shim
- `codex-acp`、Codex 原生二进制及其运行所需资源

导入时用现有 npm runtime bundle manifest 校验 package name、版本、平台 token、入口和依赖图；不允许把 Windows x64 包误用于 Windows x86 或其它平台。

## CI 与构建

新增 target-aware `prepare-runtime-seed` 步骤，位于 Tauri bundle 前：

1. 根据 target/arch 选择固定版本清单。
2. 从官方源或 Fusion 已发布镜像下载 Node、uv、Git、Codex npm 包。
3. 校验官方 SHA-256、签名（适用时）、包布局和许可证。
4. 生成 target/arch 专属 `runtime-seed` 目录和 manifest。
5. 将资源映射到 Tauri bundle；不把其它架构资源带入当前包。
6. 构建后检查安装包中存在且只有当前目标的种子。

不把二进制文件提交到 Git；CI 缓存只缓存已通过摘要校验的下载内容，缓存 key 必须包含组件版本、target、arch 和 manifest schema。

## 包体预算

以当前 `v0.1.90` 安装包和固定基线估算，压缩后的新增量约为：

| 目标 | 预计新增 | 当前包体参考 | 新包体预计 |
| --- | ---: | ---: | ---: |
| Windows x64 | 230–240 MB | 56.5 MB | 285–295 MB |
| macOS x64 | 220–240 MB | 90.7 MB | 310–335 MB |
| macOS arm64 | 205–225 MB | 86.5 MB | 290–315 MB |
| Linux x64 | 225–245 MB | 152.6 MB | 375–400 MB |
| Linux arm64 | 205–225 MB | 约 94 MB | 300–325 MB |
| Windows x86 | 0 MB | 48.6 MB | 保持现状 |

实际包体必须由 CI 对每个最终 installer/appimage/app tarball 重新统计；预算超出 15% 时构建失败并要求人工复核。

## 安全与许可

- 所有种子组件必须有固定来源、版本和 SHA-256；Git/uv 等带签名的发行物同时校验签名。
- 解包前拒绝绝对路径、路径穿越、符号链接越界和重复入口。
- 种子只读资源不能被运行中的代理写入；运行时缓存和用户配置写入受管 data 目录。
- `THIRD_PARTY_NOTICES.md` 增加 Node、Git/dugite-native、uv、Codex ACP 及其许可证和来源。
- 不在日志、manifest 公开字段或错误响应中记录任何凭证。

## 验收门禁

### 静态与构建

- manifest schema、target/arch、组件版本和 SHA-256 校验通过。
- 六个桌面目标分别只包含自身种子；Windows x86 不包含 Codex 种子。
- `cargo fmt --check`、相关 Rust/Node 静态检查、Tauri 配置解析和资源清单检查通过。
- CI 对最终包体输出大小，并验证预算阈值。

### 离线启动

在无网络、无系统 Node/Git/uv、空受管 runtime 的临时用户目录中，五个目标均能：

- 导入并启动 Node、uv/uvx、Git；
- 启动 Codex ACP 并完成 initialize/session 前置握手；
- 保留可追踪的 active inventory。

Windows x86 验收为现有在线 Version Center 路径，不要求离线 Codex。

### 故障回退

- 删除种子文件、篡改摘要、替换错误架构、破坏 Codex 入口后，启动必须回退到 Version Center。
- 在线激活失败时不删除最后一个有效旧版本。
- 应用更新和卸载不误删受管 runtime，且原有 NSIS runtime 保留策略继续通过。

### 发布验证

- GitHub Actions 六目标构建全部成功。
- 每个最终资产的种子清单、包体、签名和下载链接可复核。
- 至少在一个 Windows x64、macOS arm64、Linux x64 客户端上做真实安装、离线启动、联网回退和升级后再启动验证。

## 风险与后续决策

- `dugite-native` 的 Git 发行包需要确认运行时依赖和许可证声明，不能直接假设与系统 Git 完全等价。
- Codex npm 版本、Node 基线和 Fusion 当前 catalog 可能在实现期间变化；manifest 必须锁定版本，Version Center 负责后续升级。
- Linux arm64 当前发布矩阵和 Fusion App Release 矩阵可能只正式发布 x64；本设计要求先补齐对应桌面资产，再接入发布验证。
- 具体组件版本、CI 下载源优先级和 seed archive 压缩算法在实施计划中固定，并由一次真实构建结果更新本表预算。
