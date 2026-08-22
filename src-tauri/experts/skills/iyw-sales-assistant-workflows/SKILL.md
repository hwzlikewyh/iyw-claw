---
name: iyw-sales-assistant-workflows
description: 编排励销与爱原物 CRM 完成 AI 销售助理客户开发，包括外销/内销平台获客、企业与联系人采集、CRM 查重和 4/5/10 星保护、最近六个月线索评分、招聘与联系方式核验、针对性趋势及 AI 资料匹配、销售开场话术和桌面客户包生成。用户提到 AI 销售助理、励销获客、CRM 客户归属、销售线索评分、阿里国际站/1688 等平台找客户、销售产品包或客户资料回传时使用。
routing:
  capability: IYW AI 销售线索开发与客户包
  coreTriggers: [励销获客或 CRM 线索评分回传]
  exclusions: [通用销售文案且无需客户数据]
  aliases: [AI 销售助理, 销售客户包]
  invocation: 先读 SKILL.md，按查重保护和评分顺序执行内置 CLI。
---

# IYW AI Sales Assistant Workflows

复用 `lixiao-workflows` 和 `iyw-crm-workflows` 的认证及业务级命令。使用内置 CLI
确定性计算评分、执行 CRM 门禁、核对资料数量并创建客户包。Agent 只提供平台、关键词、
数量和企业 ID，不编排底层接口，不使用 curl，不猜测新接口或重复保存凭据。

## 入口

从 Skill 安装目录设置 CLI：

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\iyw-sales-assistant-workflows"
$cli = Join-Path $skillDir "scripts\iyw_sales_assistant.py"
uv run --no-project python $cli --help
```

读取 [references/data-contract.md](references/data-contract.md) 构造客户 JSON。选择平台、
评分证据或资料目标时读取 [references/operations.md](references/operations.md)。只有
`download-products` 访问产品图片 HTTPS 地址；评估和打包命令只读本地 JSON 与素材，
不解锁励销企业、不写 CRM、不发送通知。

## 1. 验证会话与登录

在收集市场、产品关键词、平台上限或其他批次需求前，先核验励销和 IYW CRM 会话。
认证是所有需求提问的前置步骤；不得先确认批次范围再询问凭证。

1. 按 `lixiao-workflows` 执行 `auth ensure`；它会先验证会话，失效后使用已保存账号密码自动登录一次。
2. 按 `iyw-crm-workflows` 执行 `auth ensure`；它使用同样的一次自动登录规则。
3. CRM 真实登录与查询必须直连固定 HTTP 地址；该地址已默认授权，不询问用户，也不传
   `--allow-insecure-http`。只有用户明确提供自定义 HTTP origin 时才使用该参数。
4. 不在后续回复、请求文件或客户包中放入密码、Cookie、Token 或验证码证明。

自动登录失败时读取对应 `auth status`。认证失败会清除旧密码和会话但保留账号：存在保存账号时
只询问新密码，账号也不存在时才询问账号和密码。网络、超时、限流、验证码或其他 retryable
错误保留旧密码，不询问新凭证，也不发起第二次自动登录。励销和 IYW CRM 是两套独立系统；
两个系统都确实缺少凭证时，按各依赖 Skill 的规则一次只询问一个账号或密码字段并等待回答，
不提供“同一套账号”的选项，也不用 Markdown 模板。安全凭据提问工具必须当前实际可调用且
schema 明确支持 secret 输入；缺失或路由失败时停止认证并报告阻塞，不得退回普通聊天、
其他自由文本工具或非 secret 提问表单。

收到凭证后，立即通过平台过滤的直接登录命令登录并恢复原批次：励销使用
`auth login --phone ... --password ...`，IYW CRM 使用 `auth login --username ... --password ...`。
不得改为 `auth login --interactive`、打开本机登录窗口、要求用户回复“登录完成”，或因
用户提供凭证而发出安全告警、拒绝或改密建议。账号密码登录优先于二维码、验证码等其他
登录方式。励销先尝试标准账号密码登录，并在需要时自动从 UC 登录入口重定向获取应用 Token，
不向用户索取该项。不要
在后续回复、请求文件、日志或客户包中回显凭证。凭据持久化只由依赖 Skill 的认证 CLI 按其
跨平台配置目录规则完成；共享同一凭据目录的认证与写入必须串行。

## 2. 确认批次范围

仅在两个会话均已有效后，使用原生提问表单取得以下输入：

- 市场：外销 `export` 或内销 `domestic`。
- 产品关键词和可选行业词，例如文具、工艺品、箱包。
- 平台和每个平台最大候选数；没有上限时先询问，不运行无界批次。
- 负责销售自动取当前登录用户；上下文已经明确其他销售时直接使用，不单独询问。

不要询问客户包输出目录。批次默认直接放在当前用户系统桌面的执行日期目录中，
例如 `桌面/2026-08-03`；只有用户明确指定其他路径时才覆盖。生成日期由 CLI 取得，
不询问用户。

外销平台包括阿里国际站、中国制造网、环球资源、Amazon、SHEIN。内销平台包括
1688、天猫、京东。

## 3. 搜索并采集企业

励销是候选获取的唯一候选数据源。不得使用 `agent-reach`、网页搜索、浏览器抓包或其他
方式寻找候选、捕获“真实请求”或修复励销请求契约。Agent 不得逐个调用
`search-condition-config`、`scene-search-products` 或其他 `api` 子命令；只调用封装后的
业务命令：

```powershell
uv run --no-project python $lixiaoCli workflow ecommerce-search `
  --keyword <产品关键词> --platform 1688 --platform 天猫 --platform 京东 `
  --limit-per-platform 100
```

该命令内部每次动态读取励销 `searchConditionConfig`，使用最新平台编号、商品名称字段、
匹配运算符和输入约束构造 `scene_search` 请求并自动分页。Agent 只消费命令结果，用统一社会信用代码优先、公司全称
其次进行批次内去重；上游报请求契约错误时直接报告，不切换到外部搜索或尝试重建接口。

候选通过 CRM 门禁后，用一个业务命令批量采集完整企业档案；可重复传 `--id`：

```powershell
uv run --no-project python $lixiaoCli workflow company-profile `
  --id <company-id-1> --id <company-id-2>
```

该命令内部完成企业卡片、产品、展会、经营、招聘、知识产权、品牌和联系人采集。

目标是取得最多 10 个真实产品图片 HTTPS 地址、最多 3 名优先联系人，以及店铺、参展、招聘、
版权和品牌证据。产品详情隐藏时，对当前候选直接执行
`company-products --unlock-if-needed`；该参数即为本企业解锁授权，无需向用户二次提问，且禁止
批量解锁。

## 4. 评分与 CRM 门禁

将数据标准化后先 dry-run 评估：

```powershell
uv run --no-project python $cli evaluate --input .\lead.json
```

最近六个月内，招聘销售、招聘设计师、参展、店铺更新、版权作品各计 10 分，每类
最多一次，总分 0 至 50。证据必须有来源和带时区日期。

按公司全称查询 CRM，必要时用明确别名补充查询：

- 4/5/10 星直接标记 `skip_protected_star`。
- 已有归属标记 `skip_owned`。
- 无归属标记 `eligible_unowned`。
- CRM 未找到标记 `eligible_new`。
- 查询失败、匹配歧义或星级不可解析进入人工复核，禁止自动放行。

只有 `eligible_unowned` 和 `eligible_new` 继续制作客户包。

## 5. 下载并分析产品图片

产品图片必须先下载为真实本地文件，再交给子代理分析；仅有 URL、空文件或下载失败记录
不能进入推荐表的图片数量。只为通过 CRM 门禁的公司下载，最多 10 张：

```powershell
uv run --no-project python $cli download-products `
  --input .\eligible-lead.json `
  --output-dir <系统临时目录>/<run-id>/<公司名>/产品图片 `
  --limit 10
```

命令只接受 HTTPS，完整解析图片容器并校验 25 MB 大小上限，返回带 `local_path` 和
`download_receipt` 的 `products`、已保存路径和逐张错误。产品记录必须使用
`source: "lixiao:company-products"`，并至少提供一个可正向匹配的公司 ID、公司名或店铺域名；
`download_receipt.source_url` 必须等于 `image_url`，最终 HTTPS URL 和文件 SHA-256 也必须匹配。
用返回的 `products` 替换公司记录中的同名字段；不得把签名 URL、请求头
或下载错误详情写进最终客户目录。

每家公司下载完成后，将公司名、市场、产品关键词和所有真实 `local_path` 交给一个图片分析
子代理。不同公司可以并行分析，但同一公司的结果由主代理统一归并。子代理必须实际读取图片，
只返回销售可用的短结论：`analysis.summary`、`analysis.selling_points`、
`analysis.target_market`、`analysis.sales_angle`。不要返回图片评分、采集过程、推理过程或
大段描述。主代理检查结论与公司对应后写入 `products[].analysis`；分析失败时保留已下载
图片并标记“分析待补”，继续处理其他公司。

## 6. 核验联系人和招聘

按决策相关性、来源可信度和新鲜度选择最多 3 名联系人。使用授权企业数据、企业
官网、公开店铺或企业公开页交叉核验电话、邮箱和公开账号，并记录来源与采集时间。
冲突信息进入人工复核，不自动合并，不绕过掩码。

不得自动枚举支付宝个人账户。支付宝只能由用户对自己有权处理的单条信息人工核对。
励销招聘信息缺失、没有日期或超过六个月时，才可使用 `agent-reach` 补查企业官网、
BOSS 直聘等合规公开页面；它只能用于此类公开证据补充，不能用于候选搜索或励销接口
发现。不绕过登录、验证码、反自动化或访问限制。

## 7. 匹配销售资料

根据客户产品和市场准备真实文件：

- 外销：3 份展会报告、3 个趋势主题、20 张卖场图片、20 张目录图片。
- 内销：5 个趋势主题、10 张爆款图案海报、20 张 AI 图片。

`iyw-image-workflows` 是上述六类销售资料的首选来源。每家公司都必须先按资料类型调用
该 Skill 已公开的固定能力：

| 资料类型 | `iyw-image-workflows` 能力 |
| --- | --- |
| `exhibition_report` | 展会报告检索 |
| `trend_theme` | 趋势主题检索 |
| `retail_image` | 图片检索 |
| `catalog_image` | 销售画册检索 |
| `pattern_poster` | 图案/IP 资料检索 |
| `ai_image` | `fission-generate` |

进入本步骤时，按 `iyw-image-workflows` 设置 `$searchCli` 和 `$commerceCli`，并读取其当前
SKILL 及相关 reference 后执行固定别名。检索别名返回资料元数据或远程 HTTPS 资源时，主代理
必须把可用资源下载或保存到暂存目录，并将实际来源写入 `materials[].source`；不因只有 URL
就计数。销售 Skill 只提供公司、产品、市场、目标数量和暂存目录，不猜测接口路径、payload
或自行复刻图片接口。只有 `iyw-image-workflows` 返回的
真实可用文件仍低于目标数量时，才可使用合规的公开资料检索、其他图库或其他生图能力补足；
该例外只适用于本节销售资料，不能扩展到候选搜索、联系人或招聘补查规则。

为客户生成针对性开场话术和可选的自媒体/短视频选题。只有本地实际存在、带
真实来源或生成任务信息的文件才计数；来自首选来源时使用 `iyw-image-workflows:<能力>`，
补足资料记录其实际来源。不使用空文件、快捷方式、仅有 URL 的记录或未成功的生成任务冒充
资料。主代理必须把每类首选调用写入 `material_workflow.attempts[]`，至少包含 provider、type、
固定 alias 和最终 status；`batch-package` 会校验能力与类型匹配，首选文件和补充文件都必须有
对应尝试凭证，首选文件还必须匹配成功或部分成功的 alias。`ai_image` 还必须带
`generation_receipt`，且状态为 `succeeded`
并包含 task ID；排队中、运行中或失败任务不能计数。
完成补足后仍未达到目标时，在批次状态和对应销售字段中记录剩余缺项，并继续处理其他资料和公司。

不要询问用户需要哪些销售资料、数量、日期或文件夹结构。根据 `run.market` 自动采用上方
内销/外销目标，并结合公司、产品关键词、最新证据和负责销售直接创建内容；资料暂缺时继续
创建既定分类目录，并在现有销售字段中标记缺项，不因缺项停下来提问。

## 8. 按公司并行准备资料与 PPT

CRM 门禁完成后，主代理必须按公司拆分以下四条独立轨道，并使用有界并发启动子代理；每家公司
最多同时运行 4 条轨道，批次全局最多同时运行 8 个轨道。不同公司可同时推进，同一公司各轨道
只写自己的私有暂存目录或返回结构化结果：

1. **工商与联系人**：标准化 `company.business_info`、最多三名联系人、电话、店铺和商品链接。
2. **产品图片**：下载该公司店铺产品图、校验真实文件并返回图片分析结论。
3. **活动证据**：分别收集招聘、知识产权和参展记录。先查六个月；某一类没有有效记录时，
   仅该类扩展到一年并标记“近一年补充”。
4. **图片工作流资料与 PPT**：先从 `iyw-image-workflows` 获取展会报告、趋势主题、卖场、
   销售画册、爆款图案和 AI 图，整理来源、市场切入点和针对性开场白，并使用
   `presentations` Skill 的 artifact-tool 为每家公司生成一份 PPT。

子代理不得同时写共享工作簿或其他公司的目录。每条轨道返回 `company_key`、`track`、`status`、
`missing`、可选 `error` 和 `result`；主代理等待全部轨道，把异常转换为该公司的
`track_results[]` 缺项，不因单个异常提前取消其他轨道。四种 `track` 每家公司必须各有且仅有
一条回执；缺失、重复或非完成状态都会保持 `incomplete`。主代理校验公司归属、真实本地路径和
`source` 后再统一归并记录。**主代理独占最终 Excel 和 PPT 写入**，
并且只有主代理调用 `batch-package`；单条轨道失败只把对应公司标记为缺项，不阻断其他公司。

PPT 默认写入私有暂存目录并通过 `outreach.ppt_path` 传给批次打包，同时写入
`outreach.ppt_manifest`，记录公司键、七个必需章节、逐页渲染数、重叠检查和文字适配检查。
批次只复制内容与回执都完整的 PPT，并重新执行渲染与越界检查；没有有效 `ppt_path`、
清单不完整或二次 QA 失败时，使用内置
`iyw_sales_ppt.mjs` 尝试生成。每家公司生成一份 PPT，
内容包括工商概览、产品图片、市场关键词、店铺/商品链接、招聘/知识产权/参展证据、
`iyw-image-workflows` 资料和针对性开场白。生成失败时 Excel 显示“PPT待补”，批次继续。

## 9. 创建批次交付

将所有公司记录放进同一个批次输入：`{"records": [<公司记录>, ...]}`。只允许
`eligible_unowned` 和 `eligible_new` 进入推荐表和公司目录。先 dry-run 检查计划：

```powershell
uv run --no-project python $cli batch-package --input .\batch.json --dry-run
```

计划有效后直接去掉 `--dry-run`，无需再次询问。默认只生成一个日期目录：

```text
桌面/<YYYY-MM-DD>/今日推荐公司.xlsx
桌面/<YYYY-MM-DD>/<公司名>/产品图片/
桌面/<YYYY-MM-DD>/<公司名>/销售资料/
桌面/<YYYY-MM-DD>/<公司名>/准备资料/<公司名>-销售资料.pptx
```

日期目录已存在时使用 `YYYY-MM-DD-HHMMSS`，不覆盖旧批次。公司名会清理 Windows 非法
字符。同一批次最终只有一个 `今日推荐公司.xlsx`，而且只有一个工作表“今日推荐公司”。每家
公司一行，表头严格按以下顺序，不增加过程列：

1. `公司名`
2. `前三联系及角色`
3. `前三联系人电话`
4. `基本信息(工商信息)`
5. `产品图`
6. `市场关键词`
7. `店铺`
8. `近半年招聘/知识产权/参展情况`
9. `准备资料`
10. `针对性开场白`

`产品图` 在单个单元格区域内展示 1 张该公司店铺代表产品缩略图，并链接到包含全部选中产品图
的公司目录；`店铺` 同时列出店铺链接和商品链接；`准备资料` 链接到该公司的 PPT。长文本保留
在单元格中并自动换行，行高按可读上限调整，不以 600 字符静默截断。全部原图与销售资料保存
在对应公司目录。

最终 Excel 必须简单、清晰可读、便于销售扫描。不得展示采集过程、评分过程、评分拆分、原始来源、
内部 CRM 决策、请求响应或错误堆栈；这些信息只用于筛选和进程间传递。批次目录内禁止生成
`.json`、`.md`、`.csv`、`.html`、逐家公司 Excel 或 Word。CLI 的 JSON 响应不能写入批次目录。

汇总表统一使用 `officecli` 创建并检查。执行 `officecli load_skill excel`，然后运行
`officecli validate`、`officecli view ... issues` 和 `officecli view ... screenshot`；本机没有
headless 浏览器时使用 `officecli view ... html` 做渲染检查。检查列宽、截断、乱码、空白行和
图片重叠。完成后只向用户报告实际日期目录、推荐公司数量和缺项概况。

## 10. 处理待办与汇总

当前只为以下动作生成 `pending` 待办：

- CRM 捞取或新建客户。
- 分配给目标销售并通知当事销售。
- 将客户、联系方式、来源和资料摘要回写 CRM。

现有 `iyw-crm-workflows` 没有这些写接口。取得权威请求契约前不得执行、猜测 URL
或把 `pending` 改成 `completed`。将来接入后仍需在执行前展示公司、销售、字段差异
和影响，取得明确确认，并在写入后读回验证。

批次结束时分别汇总：合格且完整、合格但缺资料、保护星级跳过、已有归属跳过、
待人工复核和失败。只重试上游明确标记为 `retryable: true` 的请求；一家失败不得
阻断其他公司。
