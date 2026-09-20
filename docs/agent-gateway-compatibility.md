# Agent 网关与模型兼容性检查

## 2026-09-21 故障与修复

0.1.218 的部分新安装机器无法使用星河，云舟和墨川则提示模型未确认。问题涉及鉴权和 ACP 模型标识两条路径。

星河原先只通过环境变量提供 Bearer 凭据。正式网关要求 `token` 请求头；鉴权失败时仍返回 HTTP 200，响应体是业务 `code: 403` JSON。内核按 SSE 读取这段 JSON，最终报告：

```text
stream disconnected before completion: stream closed before response.completed
```

成功机器的旧配置快照带有静态请求头，空配置机器缺失该字段。修复统一使用 `env_http_headers.token = CODEX_API_KEY`，启动时取当前登录态，并清除迁移配置中同名静态头。

云舟与墨川公布 `iyw-claw/<model>`，网关目录使用 `<model>`。修复在 ACP 边界转换当前值、平铺选项、分组选项、启动偏好和切换请求，继续检查实际模型，不自动换模型。

## 检查范围

| Agent | 网关鉴权 | 模型协议与本次处理 |
| --- | --- | --- |
| 星河 | 补充环境引用的 token 头 | 裸模型 ID，保留原生设置确认 |
| 远山 | 原有自定义 token 头保留 | 现场用户确认可以对话；未发现同类漏项 |
| 云舟 | 原有供应商 headers.token 保留 | 修复供应商前缀转换 |
| 墨川 | 原有 models.json headers.token 保留 | 修复供应商前缀转换 |
| 青岚 | 补充 CODEBUDDY_CUSTOM_HEADERS | 保留其他自定义头，退出账号时清除 token 行 |
| 知微 | 补充受管模型 extra_headers.token | 只写当前网关模型；配置重投影保留同地址的 token，原生登录缓存不改 |
| 逐风 | 原有 openAiHeaders.token 保留 | 供应商与模型均标记 model 类别；修复识别，优先取 id=model |
| 赫尔墨斯 | 原有 default_headers.token 保留 | 适配 0.19.0 的 models / session/set_model 协议，以及 custom:<model> 标识 |
| 月白 | 原有 providers 自定义 token 头保留 | 配置模型表使用裸 ID；静态审查未发现相同前缀问题 |
| 开放之爪 | 原有受管供应商 token 头保留 | 2026.8.2 没有公布模型配置选项；模型确认能力仍有限，未伪造确认或移除发送门禁 |
| 流光、Cursor、DeepSeek Harness、自定义 Agent | 各自原生登录或用户配置 | 不在受管网关列表，不注入爱原物 token 到第三方服务 |

赫尔墨斯的旧协议适配只使用 Agent 实际公布的模型列表；切换成功响应后才更新宿主确认值。新建、加载、恢复均接入同一转换，其他 Agent 的响应不修改。

## Windows 与诊断

- PowerShell 探测已设置 CREATE_NO_WINDOW。本次再加 Windows 专用的 `-NonInteractive -WindowStyle Hidden`，覆盖商店版启动别名重新创建控制台的情况。未在故障机器验证，不能保证所有黑框来源已消除。
- 星河重试事件保留有长度限制的脱敏原因，宿主写入主日志。新增信息在私有元数据中传递，前端 RuntimeObservation 类型不变。
- 不改变共享配置目录名称，不删除旧配置、用户技能或会话数据。

## 一手依据

- 云舟：官方 `anomalyco/opencode`，`v1.18.27` 的 `packages/opencode/src/acp/config-option.ts`。
- 墨川：官方 `svkozak/pi-acp`，`v0.0.33` 的 ACP 模型选项及切换实现。
- 青岚：官方 npm 包 `@tencent-ai/codebuddy-code@2.143.1` 中的自定义头解析器，支持换行分隔的 `名称: 值`，配置中的自定义头优先于环境同名值。
- 知微：[配置参考](https://docs.x.ai/build/settings/reference)，受管模型支持 `extra_headers`。
- 赫尔墨斯：PyPI `hermes-agent==0.19.0` 的 `acp_adapter/server.py`，模型标识包含供应商，标准配置选项更新返回空列表。
- 逐风：官方 `cline/cline` 的 `apps/cli/src/acp/acpAgent.ts`，供应商和模型选项共用 model 类别。
- 开放之爪：官方 `openclaw/openclaw`，`v2026.8.2` 的 ACP 会话展示及生命周期实现。
- PowerShell：[pwsh 参数文档](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_pwsh)，WindowStyle 仅支持 Windows。

## 验证与限制

已对照同一有效登录态：只有 Bearer 时正式网关返回业务 403；补充 token 头后，模型目录返回 8 个模型，GLM Responses 流回复 OK 并发出 response.completed。

代码交付按项目要求执行调用链静态审查、Rust 语法解析与局部格式检查、修改的 TypeScript 文件 ESLint/Prettier 检查及 git diff --check。未新增测试文件，未执行单元测试、集成测试、桌面构建或故障机器端到端测试。

合入主分支不等于安装包已更新。包含 Windows 探测和重试诊断的改动需要重新构建星河 Worker；发布后须在新安装、旧配置迁移和故障机器上复核。
