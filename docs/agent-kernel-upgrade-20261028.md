# 远山与星河内核升级

日期：2026-10-28。

## 目标与边界

远山受管 ACP 基线从 0.73.0 升至 0.98.0；该官方包固定 SDK 0.3.310，
对应原生内核 2.1.310。保留官方依赖组合，不单独替换为更新的 SDK。
星河从 0.164.0 升至 0.165.0，继续直接链接进主程序，通过同一 EXE 分流 helper。
应用源码升级不会直接更新已安装程序或 Fusion 的生产分发策略。

验收标准：

1. 官方稳定版、不可变源码提交、生产依赖及内置身份一致。
2. 保留星河现有宿主认证、图片、MCP、任务归属及 Windows/Win7 补丁。
3. 远山侧问扩展兼容新版本，并继续支持已有 0.73.0 受管安装。
4. 依赖解析、生产模块与资源、Rust/JavaScript 语法及差异检查通过。

## 执行计划

1. 已完成：核对版本与集成入口 → 官方 release、npm 清单和本地调用链。
2. 已完成：升级远山基线及星河生产补丁 → 新旧官方源码与本地补丁三方比对。
3. 已完成：依赖锁和运行时元数据同步 → 锁定依赖解析、身份与静态检查。
4. 已完成：交付审查 → 记录验证边界，保留其他未提交改动；临时清理被策略阻止。

## 官方来源

- 远山 ACP：https://github.com/agentclientprotocol/claude-agent-acp/releases/tag/v0.98.0
- npm：`@agentclientprotocol/claude-agent-acp@0.98.0`，固定
  `@anthropic-ai/claude-agent-sdk@0.3.310`、`@agentclientprotocol/sdk@2.0.2`。
- 星河：https://github.com/openai/codex/releases/tag/rust-v0.165.0
- 星河 tag object：`e374836b2728d9b7aa981c2902649e640a2edaa8`。
- 星河 source commit：`360319d8dee62d87e2d59fc0c21805d79519cfed`。

## 兼容决策

远山 0.98.0 仍提供 ACP v1 的 `runAcp`、同步 `AgentApp.connect` 和
`session.query.askSideQuestion`。宿主沿用 ACP v1 扩展入口；新版实验性 ACP v2
不属于本次升级范围。新版适配器使用 SDK 的 `unstable` 导出；侧问扩展使用同一
`AgentApp` 类注册处理器，旧版继续使用原导出。策略读取采用新版的容错语义，
日志只保留错误分类。
Fusion catalog 已从官方 npm 动态发现远山版本，无需修改历史内部构建脚本。

星河迁移保留 22 个生产覆盖包；五处源码冲突分别保留新审核截止时间导入、
任务消费目标与上游审核元数据、托管图片引用及终端图片输出。图片和终端工具
采用上游的新执行环境选择接口，MCP 取消、零执行时限及已发布连接查询保持衔接。
稳定/实验协议资源各保留可选命令描述扩展，不引入额外外置内核。

上游新增原生状态迁移 `0062_threads_attached_parent.sql`，已与官方源码逐字核对；
本次未打开运行数据库或执行迁移。已退出上游模块图的 `catalog_parameters.rs`
保留原位。既有 `tar = 0.4.46`、Windows 隐藏窗口及 Win7 系统 API 补丁保留。

## 已执行验证

- `cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1
  --filter-platform x86_64-pc-windows-msvc --locked --offline` 通过。
  Windows 图中 120 个内核包全部为 0.165.0，其中 98 个锁定官方提交、22 个本地覆盖，
  无同名重复版本；跨目标锁文件共 121 个内核包，非内核包及其依赖选择未改变。
- `harness/codex/scripts/sync-upstream.ps1 -CheckOnly` 通过，标签对象和源码提交一致。
- `node src-tauri/scripts/prepare-xinghe-worker.mjs` 通过，生成 0.165.0 进程内元数据，
  资源目录只有 `runtime.json`，无独立 worker 二进制。
- 230 个本次修改及 harness Rust 文件通过 rustfmt 语法解析；22 个覆盖包无缺失
  生产模块，34 处内嵌资源引用完整，78 个原生迁移 SQL 与官方一致。
- 上游生产接口比对覆盖 41 组函数参数数量变化及 58 组结构体字段变化；两条
  同名枚举变体误报经人工核对排除，未发现剩余适配项。此检查不能替代类型检查。
- 稳定/实验协议分别保留 20/24 处命令描述 JSON 定义和对应 TypeScript 定义。
- 远山官方 npm 包完整性核对通过；包内清单确认 SDK 0.3.310 对应内核 2.1.310。
  `node --check` 通过；静态核对侧问注册、原生 query、取消与连接关闭路径。
- 本次范围 `git diff --check` 通过；再次查询官方最新稳定版仍为 0.98.0 / 0.165.0。

## 验证边界

遵循仓库 `AGENTS.md` 与 `AGENT.md`：不新增或运行测试，不在本机编译、打包
或启动桌面端。依赖解析与静态检查不替代 CI 类型检查、链接及真实会话验收。
源码升级阶段未提交、推送、发布或替换已安装应用；用户随后授权提交、推送并合并
远程主分支。Git 交付不改变应用发布边界，远山生产分发仍由 Fusion 的受管发布流程负责。
后续需在发布环境验证桌面和服务端构建，以及新建/恢复会话、模型请求、MCP、
权限、取消、压缩、图片回读、远山侧问及各平台安装流程。

依赖解析仍有历史 `crossterm`、`appcontainer_common`、`learning_mode_windows`
未使用覆盖提示；它们不进入本次解析图。官方网页变更日志访问返回 403，本次
版本及兼容信息取自官方 GitHub release、固定源码和 npm 包。

升级经验：远山版本白名单必须与实际使用的 SDK 导出路径一起复核；仅修改版本号
会把侧问处理器注册到不同的 `AgentApp` 类。依赖重解后应比较非内核锁条目，
避免无关包的宽范围依赖被重新选择。

临时源码和检查脚本位于
`C:\Users\iyw\AppData\Local\Temp\agent-kernel-upgrade-20261028`。
已核对绝对路径及 Temp 目录边界后执行清理，自动审批仍返回 `blocked by policy`，
未提供更具体原因，因此目录保留；其中内容未加入仓库或应用资源。

## 主分支交付

按用户后续授权，仅提交本次内核升级的 206 个文件。交付前两端 `main` 均为
`12f54ff32a6c3c132f4382cabd0f10ee1685c9aa`，当前分支也位于这一基线。
通过独立升级分支提交并推送 GitLab/GitHub，再以普通合并提交整合至 `main`，
推送前复核远端，推送后逐端核验提交身份。不打发布标签或替换已安装程序。
