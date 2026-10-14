# 内置图片工具恢复与能力校验

本次优化内置 `generate_iyw_image`，不依赖原会话来自本机。生成图片需要有效登录和可消费账号。

验收标准：批项参数独立、逐项返回结果；明确失败可按候选回退；确认的结果下载/转存 502 有界重试；
生成和编辑参数完整传递；模型能力明确不支持时在生图前拒绝，未知声明保持旧目录兼容。

1. 批量 `requests` 最多 64 项，最多 16 个执行并发；每项可有不同 type、prompt、images、parameters、wait 和回退链。
   count 为 1–4 次独立执行；单个 generate/edit 的参数 n 为 1–10，受模型限制，不与 count、requests 或 fallbacks 混用。
2. 每项最多 3 个 fallbacks，参数全量替换，复用原始 prompt、参考图和蒙版。
   批量计划预算为 sum(count × (1 + fallbacks 数)) ≤ 128。
3. 对 Fusion 确认的 `upstream_image_download_failed` / `image_persist_failed` 502，
   同候选额外重试 3 次，退避 2/4/8 秒。一个批次最多 512 次实际生图调用。
   此处会重新提交生图，可能新增收费；后台对同一 URL 的下载恢复优先在原请求内完成。
4. 成功、部分成功、排队、运行、取消和状态未知立即停止；查询故障保留任务 ID。
   普通 502、网络异常和超时不自动重复生图；认证、参数等明确不可重试错误停止。
5. attempt 记录候选序号、重试序号、延迟、错误码、请求 ID 与耗时。
   批量结果保持原请求顺序，包含每项状态和错误；超过 100 个成果按既有成果登记接口分批登记。

参数包括 quality、size、background、output_format、output_compression、response_format、moderation、
input_fidelity、style、user、mask、n、stream 和 partial_images，未知扩展参数按原平台契约透传。
编辑支持 1–16 张 PNG/JPEG/WebP 参考图和 1 张 PNG 蒙版，蒙版与第一参考图尺寸一致。
多张参考图使用 multipart `image[]`，单张用 `image`；真实 MIME 和扩展名保持一致。
`parameters.mask` 支持工作区路径、HTTPS 或 data URL；也可通过 images 的 role=mask 提供，二者互斥。
当前 Fusion 完整 JSON 模式只接受 stream=false 与 partial_images=0 或省略。

`list_iyw_image_models` 返回匿名 model_ref 和安全投影的 `image_capabilities`。
代理应优先选择明确支持目标操作、透明、质量、尺寸等要求的候选，再原样复制 model_ref。
缺失/null 为未知，false 明确不支持；不根据型号或名称推断。
每个候选在预检和实际调用时共用同一元数据校验。支持约束包括枚举、数字范围、固定/自定义尺寸、
输出数量、prompt 字符、参考图与蒙版数量、单文件大小和输入格式。
客户端本身的 12000 字符、20MiB 等限制仍与模型声明取交集。
显式 transparent 的候选不能移除 transparent 或切换到不支持此参数的型号，输出使用 PNG/WebP。
未知能力允许正常透传，不作为支持承诺。非法请求返回 not_started，避免调用上游。

近期正式库错误显示，高频失败集中于上游结果图片下载/转存 502；不能据当前日志断言 DNS/TCP/TLS 的具体原因。
多个逻辑型号共用渠道，因此换型号不保证换了图片下载服务。用户提及的大象任务来自其他设备，
缺少账号/时间/任务编号，无法把该个案可靠关联到某条数据库记录。

执行与验证：隔离迁移 → 核验只含生图 diff；重试、参数及候选链 → 静态调用链审查；
目录与尺寸校验 → rustfmt 解析、模块可见性和错误分支审查；交付 → 文档 JSON 与 diff 检查。
遵守仓库限制，不新增或运行 Claw 测试，不在本机执行 cargo build/check/test/clippy；
编译验证需要允许的远端环境，本次没有调用真实收费生图或发布桌面版本。
