# Task 06：桌面包瘦身、初始化与升级保护

## 目标

从 PC 安装包移除 Skill、Node、uv、Git、Agent SDK/CLI；首次初始化和后续修复统一消费后端计划，下载到独立持久区并原子激活。应用更新不得覆盖这些内容和用户数据。

## 依赖

- Task 03 Skill plan/ticket 已冻结。
- Task 05 Agent/tool resolve 和 artifact contract 已冻结。
- Task 01 的统一 bootstrap plan 可用，或已有聚合 service 可由 Task 13 接线。

## scope_write

- `src-tauri/scripts/prepare-sidecars.mjs`
- `src-tauri/tauri.conf.json`
- `src-tauri/windows/installer-hooks.nsh`
- `src-tauri/src/commands/runtime_bootstrap.rs`
- `src-tauri/src/acp/binary_cache.rs`
- `src-tauri/src/acp/agent_storage*`
- `src-tauri/src/acp/version_center/installer/`
- 新增统一 bootstrap/inventory/download/activation 模块
- 初始化状态相关 Rust command 和非 Skill 市场 UI

## 共享文件限制

- `src-tauri/src/lib.rs`、Cargo/Node 依赖、lockfile、CI、应用根入口由 Task 13 修改。
- 需要注册新 command 时只提供导出函数和 `integration_request`。

## 包内容清理

移除：

- Tauri `externalBin` 中 uv/uvx。
- `resources/runtime` 和构建期 Node/MinGit 下载。
- 构建期 codex-acp npm prefix。
- `commands/experts.rs` 的系统 Skill `include_dir!` 由系统 Skill 迁移开关控制后删除。
- 任何 Agent SDK/CLI seed。

保留应用自身必须的 `iyw-claw-mcp` 等同版本核心 sidecar 时，必须明确证明它属于应用二进制而不是可独立管理 CLI；不能借此重新塞入整个运行时。

## 持久目录

实现并记录稳定根：

```text
app/                    应用更新区
runtime/<id>/<version>/ Node/uv/Git
agents/<id>/<version>/  Agent/SDK/CLI
skills/system/...       受管系统 Skill
skills/market/...       市场 Skill
skills/user/...         用户拥有
inventory/              manifest、active、pin、LKG
staging/                可清理临时区
config/data/logs        现有持久区
```

目录 ownership marker 包含 schema、component ID、version、artifact ID、sha、target/arch、installedAt。用户目录永不写受管 marker。

## 初始化状态机

```text
not_started -> resolving -> downloading -> verifying -> staging
            -> activating -> health_check -> ready
            -> degraded/retryable/blocked
```

- 单实例锁：同一 installation 只能有一个 bootstrap writer；其他窗口订阅进度。
- resolve 失败且有健康 inventory 时进入 degraded offline；缺必要组件时 blocked。
- 每个 action 保持 checkpoint，重启从安全边界恢复。
- 无依赖下载可并行，默认小并发；激活按拓扑串行。
- UI 显示组件、阶段、字节、速率、预计时间、重试和离线状态，不展示完整签名 URL。

## 下载器

- `.part` 与 sidecar metadata 绑定 artifact ID/sha/size/ETag。
- HTTP Range + If-Range；206 验证 Content-Range。
- 200 回退时安全重建 part；416 重新 HEAD 判断。
- URL 403/过期刷新 ticket，不改变 artifact。
- 全局并发、每 host 并发、连接/读取/总超时和指数退避有界。
- 磁盘预检包含压缩包、展开、staging 和保留旧版本空间。
- 下载完成执行 size、sha、签名，再进入解压。

## 安装与激活

1. 检查已有 immutable version manifest；完全匹配返回 keep。
2. 解压到随机 staging，限制路径、文件数、单文件和总展开大小。
3. 验证预期入口和 executable layout。
4. 运行受控 health probe；命令和参数来自客户端 allowlist，不来自服务端。
5. 原子移动到版本目录。
6. 写 inventory generation，再原子切 active pointer。
7. 会话存活时不切换该 Agent；标记 pending activation。
8. health 失败恢复旧 active/LKG，保留失败诊断并 quarantine 新版本。

## 更新保护

- NSIS 更新只替换 `app`；任何删除路径必须先 canonicalize 并证明位于 app。
- 更新前/后记录持久区 manifest 摘要，远端 E2E 比较不变。
- 不在安装器中运行 Skill reset、配置重写或运行时清理。
- App 更新若要求最低组件版本，先用 bootstrap plan 安装并验证，再切换 App 或在新 App 首启阻断使用；策略必须可回滚。
- 显式卸载默认保留用户数据；“彻底删除”是独立确认动作。

## 旧目录迁移

- 发现现有包内/旧 cache 只做一次性导入：校验版本、摘要和布局后复制到新 immutable inventory。
- 无可信摘要的旧内容只能作为 system fallback 或重新下载，不能伪装成 managed artifact。
- 用户 Skill 和 dirty checkout 不导入受管系统目录。
- 迁移 receipt 可重复读取，不重复搬运。

## 性能要求

- 启动热路径不扫描整个目录树；读小型 inventory snapshot。
- 文件哈希只在首次导入、下载验证和显式巡检执行。
- 进度事件节流，避免每个 chunk 触发前端 render。
- 初始化计划和 catalog 使用 ETag/revision；无变化不重取详情。

## 验证

本机仅执行静态审查、配置清单检查和 `git diff --check`。远端 Windows CI 必须提供：

- 安装包内容证明不含 Skill/Node/uv/Git/Agent SDK/CLI。
- 干净机器在线首次初始化成功。
- 中途断网/进程退出后续传。
- 已有正确版本零下载。
- 错摘要/签名/归档/磁盘满不会切 active。
- App 更新前后持久区、配置和记忆摘要一致。
- LKG 回滚、并发窗口和活跃会话延迟激活。

## 完成定义

- PC 包仅含应用必需字节。
- 初始化、修复、更新和回滚共享同一 inventory/installer。
- App 更新不覆盖任何用户或受管持久内容。
- 没有把外部上游直连留作正常路径。
