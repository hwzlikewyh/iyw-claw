# Node 24 运行时种子与 NSIS Smoke 隔离设计

日期：2026-08-21

## 状态

用户已确认采用固定 Node.js `24.19.0` 的方案。本文件固化实现边界，等待用户
审阅后进入实施计划。

## 问题与证据

Windows x64 安装包已经携带基础运行时种子，其中 Node.js 为 `24.19.0`，但
`0.1.97` 启动时出现“内核启动失败”。现场日志和文件探测确认了两类独立问题：

1. 种子 manifest 的 `target` 是完整 Rust target triple
   `x86_64-pc-windows-msvc`，旧客户端曾错误地与 `windows` 比较，导致合法种子
   被拒绝并回退到在线 Version Center。
2. 在线安装得到的 Node.js `24.18.1` 和 npm `11.16.0` 均可正常执行，但旧版经
   `npm.cmd` 探测时误报 `Managed tool probe returned an unexpected version`，最终
   只有 Node 状态为 `Failed`，Git 和 uv 均为 `Ready`。

现场还发现桌面、开始菜单快捷方式和产品卸载注册项被指向
`%TEMP%\iyw-claw-nsis-smoke-*`。当前 `verify-sidecar-bundle.mjs` 的安装后验证直接
以 `/S /D=...` 执行 NSIS，没有传递 test ID。即使自定义 hook 使用测试注册表，
Tauri NSIS 模板自身仍会在当前 Windows 用户下创建标准快捷方式和卸载项；进程
中断时，事后清理无法保证执行。

## 目标

1. Windows x64、macOS 和 Linux 安装包固定携带 Node.js `24.19.0`，版本和来源
   SHA-256 不随构建时间漂移。
2. 合法 runtime seed 在客户端按完整 target triple 验证并导入，成功后直接使用
   active runtime，不进入在线 resolve/download。
3. Windows npm 健康探测直接执行 `node.exe npm-cli.js --version`，不再通过
   `npm.cmd` 或 shell 包装层。
4. NSIS 真实安装 smoke 不得在普通开发机用户上下文执行，不得再次覆盖正式产品
   注册表、桌面快捷方式或开始菜单快捷方式。
5. 失败日志能够区分 seed identity、Node 版本、npm 执行和在线安装失败，保留
   预期值、实际输出摘要及退出码。

## 非目标

- 不升级到 Node.js 25，也不在每次构建时自动跟随最新 Node.js 24 小版本。
- 不移除 Version Center 在线回退和后续升级能力。
- 不把 Node.js 直接作为 Tauri `externalBin`，继续复用受管 runtime 的库存、
  staging、原子激活和回滚流程。
- 不修改 Authenticode、Tauri Minisign 或 Fusion 发布策略。
- 不在本次代码改动中自动修复当前机器已经污染的注册表、快捷方式和临时目录；
  这些属于本机状态修复，需要单独确认精确目标后执行。

## 方案选择

### 方案 A：固定种子 + 直接 npm 探测 + 一次性 runner 隔离（采用）

保留现有 runtime seed 架构，将 Node.js 固定为 `24.19.0` 及既有 SHA-256。客户端
使用编译时注入的完整 target triple 校验 manifest；Windows npm 使用包内
`node.exe` 直接运行 `node_modules/npm/bin/npm-cli.js`。真实 NSIS 安装验证只允许在
GitHub Actions 一次性 Windows runner 中执行，普通本机调用在启动安装器前失败。

该方案改动最小，保留离线首启和现有 Version Center 生命周期，同时从执行边界上
消除本机 smoke 污染。

### 方案 B：维护自定义 NSIS 模板

复制并维护 Tauri NSIS 模板，把产品注册表、卸载项和快捷方式全部改造成 test-ID
感知路径。隔离更彻底，但会长期跟随 Tauri 模板升级，超出本次修复范围。

### 方案 C：测试前后备份并恢复用户状态

在 smoke 前保存注册表和快捷方式，结束后恢复。该方案在进程被终止、runner 崩溃
或卸载器失败时仍会留下污染，因此不采用。

## 运行时设计

### 固定 Node.js 版本

`runtime-seed-config.mjs` 是 Node.js 种子的唯一版本来源：

```text
version = 24.19.0
source = https://nodejs.org/dist/v24.19.0/
artifact = target 对应官方发行包
sha256 = 仓库固定摘要
```

种子准备阶段必须同时验证版本、目标平台、归档摘要和解包布局。生成的
`manifest.json` 必须记录 `node=24.19.0`。构建后 verifier 再读取安装包中的
manifest，确认版本没有被其它环境变量或缓存覆盖。

Version Center 仍可在未来提供更高的兼容 Node.js 24 版本；已激活且有效的更高
版本不会被种子降级。这里的“固定”约束安装包离线基线，不禁止受管在线升级。

### Seed identity

manifest identity 使用以下维度：

- schema version
- 创建器标识
- Claw 版本
- 完整 Rust target triple
- arch
- platform directory

编译时由 Cargo `TARGET` 注入客户端 target triple。不得用 `windows`、`linux` 或
`macos` 这类宽泛 OS 名称代替 manifest 的完整 target。

identity 通过后，Node、Git、uv 和 Codex ACP 分组件导入。Node 成功激活后写入
active pointer 和库存；紧随其后的 runtime bootstrap 必须由 `ready_component`
短路，不再 resolve/download。

### Node/npm 健康探测

Node 版本探测执行：

```text
<runtime>/node.exe --version
```

Windows npm 健康探测执行：

```text
<runtime>/node.exe <runtime>/node_modules/npm/bin/npm-cli.js --version
```

探测成功只要求 npm 命令正常退出；npm 版本不与 Node 版本比较。Node 输出必须包含
offer/seed 的 Node 核心版本。失败 detail 包含工具、预期版本、命令文件名、退出码
和截断后的 stdout/stderr，不记录完整用户路径或环境变量。

## NSIS Smoke 隔离

### 执行边界

真实安装、卸载、快捷方式和注册表验证只在同时满足以下条件时运行：

- `CI=true`
- `GITHUB_ACTIONS=true`
- runner 为 Windows
- 安装根位于当前 runner 的 `%TEMP%`

任一条件不满足时，脚本必须在启动安装器之前返回明确错误。本机仍可执行以下无
副作用验证：安装器文件存在性、SHA-256、runtime seed manifest、sidecar 源文件
和 Tauri 配置检查。

### CI 安装流程

1. 生成 32 位十六进制 test ID。
2. 使用 `iyw-claw-nsis-smoke-<test-id>` 作为唯一临时根。
3. 使用 `/S /IYW_CLAW_TEST_MODE=<test-id> /D=<root>` 安装。
4. 验证 app、runtime seed 和 sidecar。
5. 使用同一 test ID 执行卸载器。
6. 删除本轮测试注册表键和临时根。
7. 断言 runner 中不存在指向该临时根的产品注册表值、桌面快捷方式或开始菜单
   快捷方式。

CI runner 是一次性的，但仍执行清理和残留断言，以防同一 job 的后续步骤读到测试
安装状态。不得以“runner 最后会销毁”为理由跳过清理。

## 错误处理与可观测性

- seed identity 失败记录 `expected_target`、`manifest_target`、Claw 版本和失败阶段。
- Node/npm 探测失败记录命令文件名、预期版本、实际输出摘要和退出码。
- seed 导入失败后保留在线 Version Center 回退；在线失败不得删除最后一个有效
  active runtime。
- smoke 环境不满足一次性 runner 条件时记录拒绝原因，但不得启动安装器。
- smoke 清理失败使 CI job 失败，并列出残留类型和测试根；不输出凭据或预签名
  URL。

## 验收标准

1. runtime seed 配置、生成 manifest 和安装包内 manifest 的 Node.js 版本均为
   `24.19.0`，且归档 SHA-256 与固定配置一致。
2. Windows x64 空 runtime、断网场景能够从包内 seed 激活 Node/Git/uv；Node 和
   npm 探测成功，Codex ACP 能继续初始化。
3. 有效 seed 导入后日志不出现 Node 在线 resolve/download。
4. 篡改 target、Node 摘要或 npm CLI 后，错误分别落在 identity、integrity 或
   npm probe 阶段，并按现有策略回退。
5. 普通开发机调用真实 NSIS smoke 时，在安装器进程创建前失败。
6. GitHub Windows x64/x86 安装验证完成后，不存在指向测试根的标准产品注册表、
   桌面快捷方式或开始菜单快捷方式。
7. Rustfmt、Node 语法检查、`git diff --check`、runtime seed verifier 和 NSIS
   静态检查通过。

## 风险

- 固定 Node.js `24.19.0` 后需要显式提交才能更新离线基线；这是可复现发布的预期
  代价。
- Windows x86 当前不携带 runtime seed，继续走现有在线路径。
- 一次性 runner 限制意味着普通开发机不能直接执行完整 NSIS 安装 smoke；需要
  真实本机验证时，应使用 Windows Sandbox/VM 或单独的一次性 Windows 用户，不能
  放宽生产用户隔离门禁。
