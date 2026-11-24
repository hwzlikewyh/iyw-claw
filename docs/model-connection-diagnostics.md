# 星河模型连接诊断

当星河反复报告 `Connection failed: error sending request`，仅靠重试日志无法判断连接目标和底层错误。内置工具调用使用进程内通道；模型请求仍经过内置运行时 HTTP 客户端、本机模型网关及云端转发。

## 新增记录

正常 Info 日志级别即可记录以下事件，无需用户填写 API Key 或打开全局 Debug：

| target / phase | 含义 |
| --- | --- |
| `xinghe_model_connection / route_prepared` | 本次 `/responses` 使用的协议、主机、端口、代理策略与进程代理配置是否存在 |
| `xinghe_model_connection / request_failed` | 每次失败的耗时、状态、传输错误类别、source 链类别、系统错误码与 I/O 错误种类 |
| `model_gateway / ready` | 本机监听地址及云端 origin；云端出口固定直连 |
| `model_gateway / received` | 本机网关确实收到请求，后续事件沿同一个 `gateway_request_id` span 关联 |
| `model_gateway / cloud_send_failed` | 网关发往云端时连接或传输失败 |
| `model_gateway / cloud_headers_timeout` | 等待云端响应头达到现有截止时间 |
| `model_gateway / cloud_response` | 收到云端响应头及 HTTP 状态；不代表 SSE 已完整结束 |

客户端记录实际 provider 拼出的请求 origin，不读取用户 `.codex` 文件来推测配置。HTTP 库未暴露最终套接字采用的代理，因此客户端明确记录 `proxy_route_observed=false`；`proxy_policy` 和进程代理变量是否存在只能说明配置，不能冒充最终路由。日志不再解析一次 PAC 或改变已有代理选路。

错误链最多检查 16 层，只输出白名单类别、`std::io::ErrorKind` 和 `raw_os_error` 数字，不输出原始错误字符串。未知原因标记 `unclassified`，超长链标记截断。日志不包含认证头、API Key、随机本机网关路径、查询参数、请求正文或证书身份。

新增 telemetry 包装保留原有请求遥测、计费请求 ID 和重试行为；不修改模型、认证、网络策略、超时和服务端协议。网关请求 span 在异步执行期间使用 `Instrument` 绑定，不在 await 期间持有 entered guard。

本次覆盖 `/responses` 的 HTTP 请求及网关 HTTP 转发，不追踪 WebSocket 握手或 SSE 全程。成功记录表示收到响应头，不表示整个模型生成已经完成。

## 现场采集

1. 在故障电脑运行重新构建并安装的原助理；旧二进制不会生成新增事件。
2. 保持默认 Info 日志级别，在星河发送一条消息并等待出现失败。
3. 导出该次请求前后的原助理运行日志，或将仓库 `scripts/diagnostics/xinghe` 整个目录复制到故障电脑，双击 `run.cmd` 后回传报告。脚本 1.4.0 保留新增字段，并继续使用白名单避免采集正文和凭据。

优先对照客户端 `request_host/request_port` 与网关 `listen_address`。如果出现客户端失败而没有对应时间和端口的 `received`，检查应用客户端到本机网关的连接；如果存在 `received`，继续看 `cloud_send_failed`、`cloud_headers_timeout` 或 `cloud_response`。日志缺失本身不能证明唯一根因，需要确认应用版本、日志级别和采集时间范围。

## 验证

遵循仓库约定，不新增或运行测试文件。对新增模块执行 Rust 格式/语法检查，并进行脱敏字段、telemetry 转发、错误所有权、异步 span 和原有重试/认证调用链的静态审查。编译检查结果在任务交付记录中说明；现场网络问题仍需新版二进制复现后确认。

2026-11-24：开发分支上的星河内核和共享应用库编译检查通过。合入远程主分支时按 0.171 的 `ResolvedProviderRequest` 接入诊断包装，保留主分支的 token 计数缓存与现有代理/认证路径。新增 Rust 模块格式检查及相关文件语法解析通过；PowerShell 5.1 语法解析和新增日志字段提取/脱敏的内存检查通过，未生成测试文件。未构建桌面安装包，未在故障电脑运行补丁。
