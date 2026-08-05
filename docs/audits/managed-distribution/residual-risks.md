# 残余风险（residual-risks.md）

> Audit A 只读基线 · 2026-08-01 · 未关闭风险、owner 与目标版本。
> 按“关闭规则”：无稳定复现或静态证明的标 needs-environment 并列环境与下一步；“无法复现”不算关闭。

## 1. P0（发布阻断）

| 风险 | ID | owner | 目标版本 | 状态 |
| --- | --- | --- | --- | --- |
| 硬编码 Git 凭据在源码与历史中 | IYW-SEC-001 | T00/T04/T13 | 立即（Wave H） | confirmed；工作区已删除硬编码（DB 凭据注入），待轮换+历史清理+回归 |
| Skill 下载长度语义确定性错误（raw size vs ZIP） | IYW-SKILL-001 | T00/T03 | Wave 0/1 | confirmed，待 artifact 契约 |
| 动态 ZIP 摘要与响应字节不一致 | IYW-SKILL-008 | T00/T03 | Wave 0/1 | confirmed，待冻结 artifact_sha256 |
| 系统 Skill dirty 更新破坏性 reset | IYW-SKILL-002 | T00/T04/T06 | Wave H | confirmed；2026-08-05 产品决策接受强制覆盖，BlockedDirty 门禁已移除，待回归 |
| 记忆 fallback 硬编码 Administrator 路径 | IYW-MEMORY-002 | T00/T09 | Wave H | confirmed（工作区 context.rs:84 仍未修） |
| 渠道新建/启用不连接、企微 token gate | IYW-CHANNEL-001/002/003 | T00/T08 | Wave H | confirmed；工作区已接入 reconcile、企微去 token gate，待回归 |

## 2. P1（有 owner，未清零）

| 风险 | ID | owner | 目标版本 | 状态 |
| --- | --- | --- | --- | --- |
| 组织可见性无法表达（audience） | IYW-AUTH-001 | T01/T03 | Wave 0/1 | confirmed |
| ticker 无持久状态/租约/重试/死信 | IYW-JOB-001 | T02 | Wave 1 | confirmed；T02 分支 8d48ed9 已实现租约式 jobcenter（未接线/未合并） |
| 版本无 artifact 中间态即可安装 | IYW-SKILL-006 | T03 | Wave 1 | confirmed |
| 渠道配置覆盖 protected 字段 | IYW-CHANNEL-004 | T00/T08 | Wave H/2 | confirmed；工作区已接入 config patch，待回归 |
| 渠道无回环测试/无 readiness 呈现 | IYW-CHANNEL-005/006 | T08 | Wave 2 | confirmed；工作区有 credential_ready/reconcile outcome 部分实现 |
| 记忆无 TurnComplete 采集闭环 | IYW-MEMORY-001 | T09 | Wave 2 | confirmed；工作区新增 harvest.rs，未验证 |
| UI 保存可能清除 entry marker | IYW-MEMORY-003 | T00/T09 | Wave H/2 | confirmed |
| 桌面包携带运行时/Skill、直连上游 | IYW-DIST-001 | T05/T06 | Wave 1/2 | confirmed；T06 分支 c65425b 已提交包瘦身与托管初始化（未合并）；直连上游需核实 |
| 新会话配置无统一回读/fingerprint gate | IYW-CONFIG-001 | T07/T13 | Wave 2 | confirmed；工作区新增 session_config_reconciler，未验证 |

## 3. P2（有 owner/版本）

| 风险 | ID | owner | 目标版本 |
| --- | --- | --- | --- |
| delegation/feedback 新装默认关闭 | IYW-DEFAULT-001 | T07 | Wave 2 |
| 文档仍以 /skills/download 流式代理为例 | IYW-SKILL-004 | T03 | Wave 1 |
| InstallPlanItem v1 无 artifact 字段 | IYW-SKILL-005 | T03 | Wave 1 |
| 应用更新直连 GitHub latest.json | IYW-DIST-002 | T06/T13 | Wave 2/4 |
| skill 发布源 version 与标签不一致 | IYW-SKILL-013 | T04 | Wave 1 | confirmed；T04 已提交发布校验流水线（ba3ca56，未合并） |
| 大列表无虚拟滚动 | IYW-UI-001 | T10 | Wave 3 |
| 性能无基准与观测 | IYW-PERF-001 | T12(工具)/T13(接线) | Audit B / Wave 4 |

## 4. P3（可检索 backlog）

| 风险 | ID | owner | 目标 |
| --- | --- | --- | --- |
| 管理端静态页无鉴权（纵深） | IYW-AUTH-002 | T11 | Wave 3 |
| 管理端上游 key 明文回显到浏览器 DOM | IYW-SEC-003 | T11 | Wave 3 |
| content_sha256 与 object_sha256 语义混淆 | IYW-SKILL-007 | T03 | Wave 1 |

## 5. needs-environment（无运行时证据，Audit B 关闭）
- 桌面升级删除/覆盖范围、canonicalize/junction/大小写、磁盘满、进程退出、并发初始化 —— 远端 CI/测试机。
- 下载故障注入全清单（断网、URL 过期、Range 不支持、摘要错、签名错、磁盘满、重复启动、并发计划）。
- 渠道真实账号回环（三渠道）。
- Fusion 中断/双实例/租约/泄漏 —— 隔离环境 race/benchmark/pprof。
- 离线首次安装明确阻断、已初始化离线重启可用。
- 前端 Playwright 三分辨率 + 窄屏、键盘焦点、错误状态。

## 6. 发布门禁建议
- P0 未清零前不建议发布新版本（Wave H 止损是前置条件）。
- P1 全部有 owner 与目标版本，Audit B 前须确认清零或记录真实外部阻塞（如 Task 01 未冻结导致 JOB/SKILL 系列无法修复）。
- 所有 verified 关闭项必须满足 12-health 关闭规则六条（复现/根因/非吞错/回归证据/可观测/回滚兼容）。
