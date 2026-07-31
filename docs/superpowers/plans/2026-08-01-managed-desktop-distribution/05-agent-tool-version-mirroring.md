# Task 05：Agent SDK、CLI 与基础工具镜像

## 目标

在现有 Agent Version Center 上增加后端托管的上游发现、镜像、版本兼容和强制策略，让 Agent SDK、Agent CLI、Node、uv、Git 和必要辅助 CLI 从 TOS/CDN 分发。

## 现有基础

- `agentrelease` 已有 platform/version/distribution/component/artifact/policy/event。
- Git、Node、uv 已建工具目录、版本、artifact、最低安全版本和推荐策略。
- TOS 票据、签名、不可变发布、灰度和本地库存已有部分实现。
- npm/uvx Agent 仍主要依赖外部 registry/index；自动更新仍有“版本不等即更新”路径。

## 依赖

- Task 01 契约冻结。
- Task 02 job center 可注册 mirror/discovery handler。

## scope_write

- `iyw-fusion-api/internal/domain/agentrelease/`
- `internal/application/agentrelease/`
- `internal/adapter/mysql/agent_release_*.go`
- `internal/adapter/httpserver/agentplatform*/`
- 新增 Agent/tool object storage 和签名适配文件；禁止修改共享 `tos.go`，共享 client 构造由 Task 13 接线
- 对应运维文档和聚焦测试

## 禁止修改

- SQL、router/bootstrap、桌面代码、管理前端、根依赖。

## 受管组件清单

为每项确认：稳定 ID、上游、版本规则、目标矩阵、归档布局、入口、license、摘要/签名来源和客户端能力 allowlist。

- Agent 本体：binary/npm/uvx 现有注册项。
- Agent SDK：仅已编译支持的 SDK，不允许服务端新增可执行入口。
- CLI：Codex/Claude Code 及项目实际需要的 Agent CLI。
- 基础工具：Node/npm、uv/uvx、Git。
- 辅助 CLI：wecom-cli、officecli、agent-reach 等实际被代码调用的必要工具，逐项评估是否纳入；未纳入不得默认为客户端直连。

## 上游发现任务

- 每种 upstream adapter 编译内定义 host、路径模板、认证来源和解析规则。
- 使用 ETag/If-Modified-Since，设置超时、限速、重试和缓存。
- 只发现稳定版本；prerelease 必须有独立 channel。
- 记录 upstream version、URL hash、source checksum/signature 和 license。
- 发现不直接发布，只创建 draft mirror candidate。

## 镜像任务

1. 下载到隔离临时文件，限制大小和重定向 host。
2. 校验上游签名/摘要；缺少可信摘要时按双源或人工审批策略处理。
3. 检查归档路径、入口文件、平台/arch 和恶意内容。
4. 计算项目 artifact sha，并用独立 Agent/Tool key 签名。
5. 上传 TOS 临时 key，复验后登记 immutable artifact。
6. 发布前验证 recipe 的 component allowlist 和依赖工具范围。
7. 相同 digest 幂等复用；同版本不同 digest 进入 quarantine 并报警。

## resolve 策略

- 根据 client version/channel/target/arch/org/installation bucket/local inventory 决定。
- 自动更新只允许 SemVer 更高且 policy 明确推荐。
- 推荐版本下调不自动降级。
- minimum-safe/security block 可覆盖 pin，并返回明确原因和 deadline。
- 已有兼容且健康版本返回 keep，不下载。
- PC 版本可绑定组件 min/max 范围；不兼容时阻止 App 更新或先升级组件，取决于策略拓扑。
- 任何未知 tool ID、package name、entrypoint、command 或 recipe schema 客户端必须拒绝。

## PC Release Center 协同

- 复用现有 App Release Center 的 PC version、channel、rollout、required 和 enforce-after，不另建平行版本中心。
- PC release draft 绑定一个 component policy/catalog revision；发布前验证该 release 支持的每个 target/arch 都有必要组件 artifact。
- resolve 同时回答“App 是否可更新”和“组件应做什么”。如果新 App 必须依赖尚未 ready 的组件，App offer 必须被阻断，而不是让客户端更新后失去运行能力。
- App 强制更新不等于覆盖运行时。先按拓扑完成必要组件 staging/health，再允许 App 切换；失败保留旧 App/LKG 的可启动组合。
- 记录兼容组合：client version、catalog revision、active component versions 和结果，后台可据此查看某个 PC 版本的实际健康覆盖率。

## npm/uvx 策略

短期不能把“metadata 指向公网 registry”描述成 TOS 托管。两种可接受实现：

- 构建完整可离线归档（含锁定依赖）并作为 TOS artifact；客户端不执行任意 postinstall。
- 部署专用 npm/PyPI 镜像服务，Version Center 只返回受信 mirror origin；不由 Fusion HTTP 模拟 registry。

对每个 Agent 明确选择一种，不允许静默回退公网。无法镜像的组件在后台显示未托管/阻塞原因。

## API 与事件

- 扩展统一 bootstrap resolve，或提供可被 Task 13 聚合的 agent/tool resolve service。
- 下载票据复查 artifact ready、policy、rollout 和客户端上下文。
- 客户端事件：offered/download_started/verified/activated/health_failed/rolled_back/deferred。
- 事件字段有界且不含完整 URL、本地绝对路径和凭据。

## 测试矩阵

- 上游 304、429、5xx、超时、重定向到非 allowlist host。
- 同版本换包、摘要错、签名错、归档入口缺失、license 变更。
- recommended、pinned、LKG、blocked、minimum safe、rollout、client incompatible。
- Windows x86_64/arm64 以及 schema 已支持的其他 target。
- npm/uvx 完全离线安装验证，不产生公网请求。

## 验证

- 运行 agentrelease 领域、application、repository 和 HTTP contract 定向测试。
- 用 fake upstream/TOS 验证发现、镜像、同版本换包 quarantine、fencing 和发布不可变。
- 在隔离网络环境确认 npm/uvx 受管方案不访问公网 registry/index。
- 输出每个已纳管组件的 target/arch、来源、摘要、签名、license 和 ready artifact 矩阵；存在空洞时不得宣称完成。
- 检查本任务没有修改共享 router/bootstrap/`tos.go`，接线请求已交 Task 13。

## 完成定义

- 后端可定期发现和镜像所有明确纳管组件。
- resolve 完全由 policy 决定，不再使用“版本不等即更新”。
- 大字节由 TOS/CDN 承担；未知执行能力无法由后台注入。
- 每个组件有来源、许可证、摘要、签名、兼容和回滚证据。
