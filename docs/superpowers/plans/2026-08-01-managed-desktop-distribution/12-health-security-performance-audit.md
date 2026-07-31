# Task 12：全项目健康、安全与性能审计

## 目标

系统排查三个仓库的现存问题，建立可追踪缺陷台账并验证修复闭环。此任务不是“扫一遍 TODO”，而是以架构边界、关键用户路径、故障注入和发布产物为单位形成证据。

## 执行方式

分两轮：

- Audit A：实施前，只读基线，确认已知问题、发现新问题、分派 owner。
- Audit B：Task 02-11 合并后，回归审计，确认没有跨任务回归并关闭缺陷。

本任务默认只写审计报告、诊断脚本/fixture 和基准结果。业务修复必须回到对应 owner task；跨域修复交 Task 13。

## scope_write

- 新增 `docs/audits/managed-distribution/` 台账、证据索引和报告
- 非生产的静态扫描/契约检查/基准脚本，前提是不新增依赖或改根配置
- 不直接修改业务代码

## 已知缺陷种子

以下必须先录入，不代表完整清单：

| ID | 严重度 | 问题 | Owner |
| --- | --- | --- | --- |
| IYW-SKILL-001 | P0 | raw size/空对象摘要被当作动态 ZIP metadata，安装必然失败 | T00/T03 |
| IYW-SEC-001 | P0 | 系统 Skill Git 链路存在硬编码凭据，需移除和轮换 | T00/T04/T13 |
| IYW-SKILL-002 | P0 | 系统 Skill dirty 更新可能破坏性 reset 用户修改 | T00/T04/T06 |
| IYW-SKILL-003 | P1 | 系统 Skill/市场/用户目录所有权混杂且包内嵌 | T03/T04/T06 |
| IYW-AUTH-001 | P1 | public/private 无法表达同组织所有登录用户可见 | T01/T03 |
| IYW-JOB-001 | P1 | ticker 无持久状态、多实例租约、重试和死信 | T02 |
| IYW-CHANNEL-001..006 | P0/P1 | 新建/启用不连接、企微 token gate、配置覆盖、无 E2E | T00/T08 |
| IYW-MEMORY-001 | P1 | 没有 TurnComplete 自动采集闭环 | T09 |
| IYW-MEMORY-002 | P0 | fallback 硬编码 Administrator 用户目录 | T00/T09 |
| IYW-MEMORY-003 | P1 | UI 丢诊断字段并可能清除 entry marker | T00/T09 |
| IYW-DIST-001 | P1 | PC 包仍携带运行时/Skill，客户端仍直连多上游 | T05/T06 |
| IYW-CONFIG-001 | P1 | 新会话配置未形成统一回读/fingerprint gate | T07/T13 |
| IYW-DEFAULT-001 | P2 | delegation/feedback 新安装默认关闭 | T07 |

## 审计范围

### 身份、授权和多租户

- 所有用户/管理路由鉴权是否一致。
- org_code、owner、official、private 在 list/detail/dependency/download 是否一致。
- Snowflake ID 是否在浏览器被 Number/parseInt。
- 跨组织枚举、错误侧信道、缓存 key 缺组织维度。

### 制品供应链

- 上游 allowlist、重定向、摘要、签名、license、同版本换包。
- TOS object key/短时 URL 泄露、URL 日志和缓存。
- ZIP/TAR 路径穿越、符号链接、设备文件、炸弹、重复路径。
- 发布后可变字段、artifact 覆盖、staging/孤儿清理。

### 桌面升级和文件系统

- 安装/更新/卸载的删除和覆盖范围。
- canonicalize、junction/symlink/reparse point、路径大小写、磁盘满。
- app/runtime/config/data/logs/skills/agents ownership。
- 并发初始化、进程退出、锁、原子 rename、LKG。

### 会话与配置

- 所有 spawn 入口是否经过 storage、Skill、provider、config gate。
- 新建/恢复/probe/渠道/subagent 的代际语义。
- 配置并发写、格式损坏、用户字段保留、凭据脱敏。
- MCP tool 暴露与实际 host bridge readiness 是否一致。

### 消息渠道

- 每个 provider 的 credential、connect、poll/webhook、dedupe、echo suppression。
- dispatcher backpressure、workspace、Agent、bridge、prompt、reply、outbox。
- 消息/附件大小、富文本降级、权限交互、重启恢复。

### 用户记忆

- root 解析/迁移/只读/事务恢复。
- tool authorization、turn nonce、迟到写、candidate 上限和去重。
- UI 隐藏 marker 与保存一致性、敏感内容策略和日志。
- self-improving Skill 是否有旁路文件写入。

### 后端并发与可靠性

- MySQL 5.7 事务和索引、任务 claim/fencing。
- goroutine/ticker 生命周期、queue 上限、shutdown drain。
- 外部 HTTP/TOS/Git 超时、重试、连接复用、body close。
- catalog revision、缓存旧版本、TOCTOU。

### 前端/UI

- 加载/空/错误/partial/offline/stale 状态。
- race：旧请求覆盖新请求、重复点击、卸载组件后 setState。
- 大列表、文件树、进度事件的渲染性能。
- 可访问性、长文本、窄屏、无重叠和按钮语义。

### 性能

- Fusion 不代理大文件后的出口字节验证。
- 启动、bootstrap、resolve、下载、安装、spawn、Skill 列表/详情 P50/P95/P99。
- 内存、句柄、临时文件、goroutine/task 泄漏。
- 64KiB/1MiB API payload 和大小不同 artifact 的阶梯测试。

## 静态检查

- secret scan：当前树和历史；结果只报路径/类型，不回显值。
- 外部 URL/registry/git source 清单与 owner。
- `include_dir!`、`include_bytes!`、Tauri resources、sidecar 和安装包清单。
- `unwrap/expect/panic` 位于用户输入/网络/文件路径的风险点。
- ignored Result、空 catch、无限/无上限 retry、无 timeout。
- 超大函数/文件、共享状态锁跨 await、阻塞 IO 在 async。
- SQL 字符串拼接、未参数化查询和缺索引查询。
- 日志中的 token/password/config/message/base64/image。

静态命中必须人工确认，不把数量当成漏洞数量。

## 动态与故障注入

- Fusion 可在隔离环境跑 unit/contract/integration/race/benchmark。
- 桌面只在远端 CI/测试机运行，当前机器禁止编译和启动。
- 三渠道真实账号回环。
- TOS 403/404/429/5xx、慢流、截断、Range 异常、摘要错。
- MySQL/TOS/Git 中断、worker crash、双实例、租约过期。
- Windows 更新、被占用文件、防病毒延迟、磁盘满和路径含 Unicode/空格。
- 离线首次安装应明确阻断；已初始化离线重启应可用。

## 关闭规则

缺陷只有同时满足以下条件才可 verified：

1. 有稳定复现或明确静态证明。
2. 根因定位到文件/符号/状态转换。
3. 修复不只是吞错/放宽校验。
4. 有失败前、成功后的回归证据。
5. 相关日志和指标能观察。
6. rollback/compatibility 已评估。

“无法复现”不是关闭；标为 needs-environment 并列环境和下一步。

## 交付

- `inventory.md`：仓库、入口、外部依赖和关键数据流。
- `defects.yaml`：唯一缺陷准数据源。
- `security.md`、`performance.md`、`reliability.md`。
- `verification-matrix.md`：命令、环境、结果和证据链接。
- `residual-risks.md`：未关闭风险、owner 和目标版本。

## 验证

- 对每个审计类别抽查 evidence 能回到真实命令、日志、截图、测试或源码位置。
- 校验 `defects.yaml` ID 唯一，P0/P1 均有 owner、复现、根因和回归证据。
- 将外部 URL、凭据、包内容和共享入口清单与实际仓库再次 diff，确认没有遗漏新增路径。
- Audit B 重跑 Audit A 的所有自动检查并比较差异；新增告警必须人工分类，不能直接忽略。
- 桌面动态证据必须来自远端 CI/测试机，报告中明确环境和 commit；不得在当前机器违反桌面执行限制。

## 完成定义

- 所有审计类别有 evidence，不留空白“已检查”。
- P0/P1 清零或因真实外部阻塞明确停止发布。
- P2 全部有 owner/版本，P3 有可检索 backlog。
- Audit B 证明集成后关键路径无回归，不能用“静态看起来没问题”代替目标环境 E2E。
