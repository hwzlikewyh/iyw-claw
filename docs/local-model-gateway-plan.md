# 本地模型网关实现方案

日期：2026-10-17。用户已授权写出方案后直接实现。

## 目标与验收

1. 受管 Agent 请求经过 PC 内置 OpenAI/Responses/Messages 兼容服务，再到 Fusion；云端鉴权、路由和计费保持权威。
2. 本地只转换真实图片结构块，保持原图字节、detail、cache_control、工具关联和未知字段；不执行视觉分析或生成文字摘要。
3. 图片通过受限签名票据从 PC 直传 TOS；后续携带受管 ID 或 HTTPS URL，重复图片查重，不再经 Fusion 上传 Base64。
4. 纯文本不构建 serde_json::Value 树，响应流直接透传；内存、图片准备并发、连接和缓存均有界。
5. 请求取消、错误、SSE、启用的 WebSocket、主进程启动/退出与多账号闭环；生图 MCP 的独立入口明确接入。

## 方案选择

选择现有 Rust/Axum 运行时内的轻量同协议代理。相比单独打包 Node/Go 服务，减少进程、分发与内存；相比复制完整 Fusion，避免本地渠道密钥、价格、协议转换和故障切换分叉。

复用 cc-switch 的 Axum 生命周期、连接池和 Body::from_stream 模式；参考 new-api 的请求可重放与 multipart 边界、sub2api 的输出提交后禁止重放。实现独立编写，不复制外部项目源码。

参考快照：cc-switch 797e0276、new-api 09109ec3、sub2api b0a02a33；本仓 Fusion 的图片资源、协议重写和计费为集成依据。

- [cc-switch Axum 服务](https://github.com/farion1231/cc-switch/blob/797e0276ffc20e54cbdf55e3de84505d77231c91/src-tauri/src/proxy/server.rs)：复用主进程和流式响应的模式。
- [new-api OpenAI adapter](https://github.com/QuantumNous/new-api/blob/09109ec315a5086366f94bcce00f177ec831dbe1/relay/channel/openai/adaptor.go)：区分 JSON 和 multipart 契约。
- [sub2api 图片直传](https://github.com/Wei-Shaw/sub2api/blob/b0a02a3304a0a78cdd003ddda1dd23ba02c03084/backend/internal/service/openai_images_direct.go)：区分 URL/文件输入及提交边界。

OpenAI Docs 官方站点在调研时无法取得正文，接口类型补充核对公开官方 SDK；未把失败的页面抓取当成已验证依据，来源详见 Obsidian 同日调研记录。

## 架构与数据流

```text
受管 Agent / 生图 MCP
 -> 主进程 loopback 网关（动态端口、随机路径凭据、请求 token）
 -> 请求体接收：256 KiB 内存，超出匿名临时文件
 -> 图片结构扫描，记录字符串范围，不反序列化整张图/长历史
 -> 按账号和编码内容摘要查询元数据缓存
 -> 缺失：流式 Base64 解码、SHA-256、MIME 嗅探，原字节临时文件
 -> Fusion images/input/prepare init（仅摘要、MIME、大小）
 -> PC PUT TOS 私有对象（精确长度、摘要、不可覆盖签名）
 -> Fusion complete（HEAD 元数据核验）
 -> 图片输入和编辑 JSON 替换为受管 image_id 引用
 -> Fusion 按实际候选能力传 URL 或保留其明确兼容行为
 -> 上游响应字节流 -> 本地字节流 -> Agent
```

历史仍由 Agent 管理，本地网关不改写 SDK 历史文件。缓存保存账号哈希、编码哈希、image_id、短期 URL 和到期信息，不保存 token、原图或 Base64。当前仅有进程内缓存；重启后重新解码计算原字节哈希，由云端 init 查重，已有对象不重传。

## 性能边界

| 资源 | 初始约束 |
| --- | --- |
| 请求接收内存 | 每请求 256 KiB，超过后溢写匿名临时文件 |
| 图片解码缓冲 | 64 KiB，不解码成像素、不缩放、不重新编码 |
| 全局并发请求 | 64，饱和立即明确返回 503，不无限排队 |
| 图片准备 | 2 路，全局有界；相同内容合并工作 |
| 本地阻塞计算 | 扫描/哈希/解码/拼接共用 2 路许可，许可随任务而非 HTTP future 释放 |
| 请求体上限 | 128 MiB，保留错误可诊断性 |
| 单图原字节上限 | 沿用 Fusion 32 MiB |
| 图片引用数 | 单请求最多 128，超出明确失败 |
| JSON 嵌套 | 64 层，每对象最多 256 个字段；大字符串仅保留 1024 字节前缀 |
| 内存资产缓存 | 最多 512 条元数据，按账号和内容隔离 |
| WebSocket | 最多 4 条连接，单消息/帧最多 8 MiB |
| 响应 | 不收集整段 SSE，不解析每个 token，不在本地再次核算 usage |
| HTTP | 长生命周期连接池，禁重定向，保留取消和背压 |

上述为实现的资源预算，不是未经测试的内存/延迟保证。性能验收比较纯文本 64 KiB/1 MiB、单图/多图首次与命中缓存的耗时、RSS/峰值、CPU、Fusion 字节与 TOS 字节。

256 KiB 是接收缓存阈值，不是整个请求或进程的 RSS 上限；网络库收到的 chunk、TLS、JSON 前缀、WebSocket 帧及文件页缓存还会占内存。HTTP 请求先完整接收到内存/临时文件再转发；纯文本有一次字节预筛，命中图片标记才做结构扫描，不宣称请求零缓冲、零拷贝。图片缓存命中跳过 Base64 解码和上传，但仍需扫描/编码摘要。

超出阈值以磁盘换内存：最坏 64 × 128 MiB 的输入临时文件约 8 GiB，另有两路解码/改写文件；匿名文件正常关闭自动删除，空间不足明确失败，不能宣称完全不占磁盘。64 并发包含响应持续时间，图片队列也受此上限约束，不另建无限任务队列。

## 协议与路由

本地允许 GET/POST 的固定模型相关路径：models、chat/completions、responses（含压缩相关子路径）、messages（含 count_tokens）、images/generations、images/edits、images/input/prepare、image-analysis。

统一通过受管 provider overlay 返回本地地址；上游地址单独保留原有环境/测试/生产规则，禁止递归请求自己。Messages 保留 anthropic 前缀，OpenAI 使用 v1；生图 MCP 原有固定云端入口接本地同一服务。

用量看板、消费记录查询、记忆 embeddings/rerank 保持直连稳定云端地址，不受本地 allowlist 影响；记忆向量空间与已确认账单的缓存身份不会因动态端口在每次启动变化。

普通 JSON：Responses input_image、Chat image_url、Messages image/source，以及 tool_result 的嵌套图片按结构处理。只替换图片源字段范围，原始 JSON 其余字节尽可能不动；大字符串不加载为 String。

已有 image_id 透传。HTTPS 源不下载。multipart 有界接收后按原文件契约转发，**本轮不将 multipart 文件转成 URL，因此这类请求仍占 Fusion 图片带宽**。SDK 通过 prepare 的 upload 请求也在本地转换为直传，返回原资源响应契约。

编辑兼容 JSON 的 `images[].image_url` 和 `mask.image_url` 转为受管引用；Fusion 校验所有者后先改 HTTPS。沿用既有 400/415/422 格式拒绝时的 JSON→multipart 回退，仅在该阶段从自己的受管对象读取原字节，不下载任意客户端 URL。读取总原图受 32 MiB 限制，multipart 最终仍受原有 32 MiB 限制。此兼容分支节省 PC→Fusion 入站，不能节省 TOS→Fusion→上游的原图流量；没有重复调用成功生图的本地重试。

WebSocket 用独立双向桥接，上传准备不阻塞上游响应接收，转发子协议及关闭码/原因；文本请求帧经过同一转换，二进制帧原样传递。8 MiB 是明确兼容边界，超限不分割消息、不承诺适用于所有长历史；Fusion 未开放对应 WebSocket 路由时保留其非 101 状态，由 SDK 按原有策略决定是否使用 HTTP。本轮不为 Fusion 新增 WebSocket 服务，也未实测系统代理兼容性。

## Fusion 配套

1. 恢复新 imagetransfer 包装器丢失的 InputImageStore，避免 prepare 503 和客户端回退 Base64。
2. 不改变现有 InputImageStore，新增可选的直传扩展端口；没有该能力时旧客户端/旧接口仍按原规则工作。
3. images/input/prepare 增加 init/complete：身份从云端认证上下文取；对象键沿用组织+用户+内容摘要；init 签名私有 PUT，绑定 MIME、长度、摘要、不可覆盖、短有效期；complete 只 HEAD 核验，不下载原图。
4. 对象存储 SDK 校验上传摘要；metadata 声明不作为实际字节哈希的替代。私有 ACL、owner 和 hash 绑定，不信任客户端任意 image_id。
5. URL 生图入口允许图像模型准备同一图片资源；仅有图像模型而无文本候选时也可查重/直传。
6. 本地发 `x-iyw-native-vision: true`：文本模型收到图片时明确失败，不自动调用另一个模型摘要。未接本地网关的旧客户端保留原有行为，显式 analyze_image 工具不在本轮移除范围。
7. 不修改数据库、依赖、价格、生产配置或执行部署。

### prepare 直传契约

同一入口 `POST /v1/images/input/prepare`、同一用户 token；身份来自云端认证，不接受 body 伪造组织/用户。

```json
{"operation":"init","model":"实际模型 ID","sha256":"64 位小写原字节摘要","mime_type":"image/png","size_bytes":12345}
```

已有对象返回 `supported=true/found=true/image_id/image_url/sha256/mime_type/size_bytes`；缺失返回 `supported=true/found=false/image_id/upload:{url,headers}`。PC 按票据向 HTTPS 执行 PUT，不携带 Fusion token；HTTP 409 后继续 complete 核验。将 operation 改为 complete 并提交同一描述符，返回核验资源；存储/大小/摘要不符不提交模型请求。旧 lookup/upload/resolve/read 不改契约。

## 安全、错误和生命周期

监听 IPv4 loopback，Host 必须匹配动态 authority；拒绝浏览器 Origin，不开放 CORS，不提供任意代理目标。随机路径只是附加本机隔离，不能代替 token 鉴权；不输出到日志。

2026-10-17 用户补充要求：PC 网关必须沿用 Fusion token。入口只接受当前桌面登录账号的同一个 access token，兼容 `token`、`Authorization: Bearer` 和已有 SDK 的 `x-api-key`；不签发本地替代 token。缺失、错误、过期、未登录和空间切换中均在收体/处理图片前返回 401。登录/续期/退出/空间会话落库后同步更新内存凭据摘要；启动仅从当前持久会话初始化一次。热路径不查库、不新增网络鉴权请求，最终撤销状态、权限与计费仍由 Fusion 判断。HTTP 完成准备后和 WebSocket 后续帧再次检查，旧连接不能使用旧凭据提交新请求；已提交云端的生成不重放。

本轮补充步骤：鉴权状态与账号生命周期接线 -> 静态检查；统一 HTTP/WebSocket 入口及准备调用校验 -> 沿失败路径复核；用户明确授权本机打包 -> 运行现有 Windows 生产构建入口，使用无签名本地候选包，不发布、不上传。验收为本地 401、会话变更同步、正常 token 继续到 Fusion，以及完整构建结果可追溯。

请求错误只有脱敏 code/message/阶段和字节统计。普通模型 HTTP 响应保留云端 status、request_id、usage 和错误正文；prepare 内部调用保留 HTTP 错误状态，但以固定脱敏消息替代其正文。成功转发字节统计为 debug 日志，避免每轮 info 噪声；不记录签名 URL、header 或图片。

本地关闭自动 HTTP 重试，不重试模型调用。图片上传允许去重检查；未知生成结果不得重新提交。主进程停止可取消异步准备/上传/转发；响应消费者断开后响应流被释放。已经开始的同步扫描/解码不能被 Tokio 强行中止，最多保留两路，完成后释放文件；**HTTP 完整收体后、返回响应头前的客户端断开没有独立检测，云端调用可能继续到超时**。这部分取消能力须在后续允许的运行环境验收，不能承诺所有阶段立刻停止。

随主进程内置 MCP 服务启动并公布地址，受管会话配置通过既有 reconciler 读取；进程关闭撤销地址、取消异步请求并共用 5 秒关闭期限。动态端口不硬编码到发行配置。监听器异常退出保持失败，不静默回退 Base64 云端大包，需要重启主进程服务。若内置服务根本未成功启动，原有 provider 云端回退仍可能被使用；必须将启动成功作为发布门禁。

账号缓存 key 使用 token 与编码摘要的 SHA-256，不保存 token；token 更新会丢失本地命中但不会串号。最终资源归属始终由 Fusion 的组织+用户作用域验证。没有新增本地供应商 key、余额或价格缓存。

## 文件与实施步骤

1. Fusion 资源接口修复和直传扩展 -> Go 定向回归、格式、相关编译检查。
2. Rust model_gateway 模块：生命周期/目标地址/鉴权/转发/临时请求体 -> 逐调用链静态审查。
3. JSON 范围扫描与图片直传缓存 -> 检查大字符串、转义、边界、重复和错误释放。
4. Agent provider 与主机生图接线、prepare upload 兼容、WebSocket -> 检查桌面/服务器启动与退出顺序。
5. 文档、静态格式/长度/diff 和独立审查 -> 保留未验证项，完成源码交付。

同时遵守仓库禁止本机编译/启动桌面端及默认不新增桌面测试的规定；不会把静态检查写成运行测试通过。真实 TOS、上游视觉与 GUI/性能测量需要后续允许的验证环境。

## 当前状态

- 参考和设计：已完成。
- Fusion 配套：源码已实现，正式回归保留。
- 本地网关和接线：源码已实现；桌面和独立服务端共用现有主进程生命周期。
- 源码交付文档与最终检查：已完成；线上/桌面端到端验收尚未完成，不标记可直接上线。

### 已执行验证

- Fusion `go test ./... -count=1`、`go vet ./...`、`go build ./...` 通过；包含三类 LLM 入口非流式/流式 smoke。
- 最后测试整理后再次执行 relay/objectstorage/imagetransfer 三包 test/vet/build 通过。
- 聚焦覆盖：包装器接口委托、私有不可覆盖签名、HEAD 完成不读图、init 已有对象校验、图像模型准备、用户隔离、Messages source 未知字段保留、三入口禁止摘要、edits URL 被拒后原字节 multipart 和消费记录。
- Rust 新模块通过独立临时 Cargo 工程类型检查，仅引用真实新模块、对主项目两处调用做 stub；不是完整桌面编译。没有新增/运行桌面测试、打包或启动桌面。
- 新模块 rustfmt、行数/函数边界与本轮 diff 检查；既有大文件不借机重构或整体格式化。
- requesting-code-review 技能要求的只读审查代理多次收到空任务，未形成独立结论；主代理完成静态复核，不声称独立审查通过。
- 清理本次临时 Cargo 工程的操作被工具安全策略拒绝，未绕过。残留仅为 `C:/Users/iyw/AppData/Local/Temp/iyw-local-gateway-check-f65d7fe20e76430db6fdf129bfc28a2b` 中的 Cargo.toml/main.rs/Cargo.lock/target；无图片、token 或业务数据，待用户或允许的维护流程删除。

### 性能数据与发布门禁

已执行 Fusion 既有 64 KiB/1 MiB 六方向转换和流式端到端基准。该样本验证云端入口可运行，**不是本地网关压测，也没有改动前同环境基线**；不推导 CPU/RSS 降幅或“不回退 5%”。真实低占用目标未完成验收。

1. 先发布 Fusion，核验带 imagetransfer 包装器时 prepare 不再 503；真实 TOS 验证 PUT、错误摘要/大小拒绝、不可覆盖、complete HEAD，以及跨账号拒绝。
2. 在允许的远端 CI 构建桌面/服务器二进制；核验主进程启动顺序、动态端口覆盖、休眠恢复、关闭与现有会话重新连接。
3. 选已验证原生视觉的实际模型，分别验证三种协议、工具结果图片、细字原图、历史重放和压缩后重读；不支持视觉的模型应明确失败。
4. 按候选协议开启现有 URL 能力，保留未验证渠道的 inline/multipart 兼容。此操作为生产配置变更，本轮未执行。
5. 测量纯文本 64 KiB/1 MiB、1/8/32 MiB 图片首传/重复/并发、取消和磁盘满的 P50/P95、CPU、RSS/峰值、磁盘临时量、Fusion 入/出站及 TOS 字节；检查 WebSocket 8 MiB 限制是否满足真实会话。

本轮明确不覆盖：流光等未走受管 provider 的 Agent、任意 SDK 自定义 endpoint、multipart 减流、二进制 WebSocket 图片识别、持久本地缓存、生产部署。附件最初由 SDK 读取/压缩的行为仍属原 SDK；网关不能把已被别的工具总结成文字的消息恢复成原图。

### 用户追加的本机打包验证

用户在补充 token 鉴权后明确要求本地打包，因此本次可以运行完整桌面构建；未授权安装、上传、签名发布或改生产环境。使用现有 `build-desktop.mjs --jobs=2 --no-sign`，环境 helper 已编译成功。首次默认前端构建在 `globals.css` 的 PostCSS worker IPC 上触发 Turbopack 超时，尚未进入 Rust 主程序编译。

采用 Next.js 已有 `--webpack` CLI 选项重建当前源码前端（`pnpm build --webpack`），不改根配置、版本、依赖或锁文件；通过后才允许 `--reuse-assets` 继续桌面构建，不能将旧 out/ 直接包装。

2026-10-17 续作已取回上一轮结果：Webpack 退出 0（编译约 90.1 分钟），TypeScript、39 个静态页面及导出完成。静态生成期间出现 `window is not defined` 日志但未使构建失败，保留为运行验收风险；不能以构建成功证明该页面运行正确。`cargo check --locked --lib --target x86_64-pc-windows-msvc --jobs 1` 退出 0（87m40s），包含本地网关和 token 鉴权，存在既有未使用代码等警告。

用户已同意继续并允许必要的 `.next` 清理；由于原构建最终成功，实际未清理。检查 src/public 与前端配置均无晚于本次 out/ 的输入后，运行 `build-desktop.mjs --jobs=2 --no-sign --reuse-assets`，整体退出 0。Release 编译成功（89m37s），NSIS 封装、内嵌星河身份、环境 helper 和主程序内置执行器身份校验均通过；身份查询未启动 GUI。另执行安装包体积门禁通过，118.3 MiB，低于 620 MiB 上限。

本地候选包：`D:/projects/iyw/iyw-claw/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/原助理-v0.1.272-x64-setup.exe`，124,041,283 bytes。
SHA-256：`4084780162cab0c3ddc0dbba929808c955820f81d324a63436e2aa5624d9dc74`。
主程序产品版本 0.1.272、230,334,976 bytes；版本号保持不变。Authenticode 状态 `NotSigned`，没有 updater 签名，不作为正式发布包。

Cargo.lock 与 pnpm-lock.yaml 校验和均与打包前一致；本轮 rustfmt 和定向 diff 检查通过。未新增/运行测试套件、安装应用、启动 GUI、上传或部署。完整编译/封装通过不等于真实图片、401 动态场景及性能验收通过；正式使用本地图片直传前仍需先部署匹配的 Fusion 配套，静态生成 `window is not defined` 日志仍待运行核验。

## 2026-10-19 正式主分支集成

用户已明确授权提交、推送并合并 PC 网关到 `main`。使用新的隔离工作树，
从最新远程主分支整合此前候选包的 17 个网关模块与 11 处接线；不包含旧工作树
其他改动、版本号、打包产物、依赖、根配置、数据库或已由主分支修复的导入补丁。

验收与执行：

1. 核对生图栈溢出已有修复与实际分支包含关系，不重复改动其实现。
2. 检查网关生命周期、Fusion token 同步、模型与生图入口、账单和记忆云端地址。
3. 完成主代理静态审查、Rust 新模块格式/语法和 `git diff --check`；不运行测试。
4. 普通合并并推送 `main`，分别回读 GitLab 和 GitHub；保留所有原工作树与用户改动。

本轮遵循仓库本机执行限制，不重新编译、打包或启动桌面端，不调用收费模型。
此前 0.1.286 完整构建结果仅适用于当时源码；本轮主分支集成后的完整构建与运行
验证由正常发布环境完成。未手动触发发布、部署 Fusion 或修改生产配置。

当前安装的 0.1.287 不含网关。合并源码不会改变已安装程序；需要包含本次提交的
后续安装包才能启用。本地图片缓存只按 token 与编码摘要隔离；沿用同一 token
切换空间后的资源复用仍依赖云端所有者检查，真实跨空间场景尚未验收。
