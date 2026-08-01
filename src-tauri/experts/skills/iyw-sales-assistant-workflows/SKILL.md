---
name: iyw-sales-assistant-workflows
description: 编排励销与爱原物 CRM 完成 AI 销售助理客户开发，包括外销/内销平台获客、企业与联系人采集、CRM 查重和 4/5/10 星保护、最近六个月线索评分、招聘与联系方式核验、针对性趋势及 AI 资料匹配、销售开场话术和桌面客户包生成。用户提到 AI 销售助理、励销获客、CRM 客户归属、销售线索评分、阿里国际站/1688 等平台找客户、销售产品包或客户资料回传时使用。
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
评分证据或资料目标时读取 [references/operations.md](references/operations.md)。CLI 只
读本地 JSON 和素材文件，不访问网络、不解锁励销企业、不写 CRM、不发送通知。

## 1. 验证会话与登录

在收集市场、产品关键词、平台上限或其他批次需求前，先核验励销和 IYW CRM 会话。
认证是所有需求提问的前置步骤；不得先确认批次范围再询问凭证。

1. 按 `lixiao-workflows` 执行 `auth status` 和 `auth ensure`。
2. 按 `iyw-crm-workflows` 执行 `auth status` 和 `auth ensure`。
3. CRM 真实登录与查询必须直连固定 HTTP 地址；该地址已默认授权，不询问用户，也不传
   `--allow-insecure-http`。只有用户明确提供自定义 HTTP origin 时才使用该参数。
4. 不在后续回复、请求文件或客户包中放入密码、Cookie、Token 或验证码证明。

认证检查显示未登录或会话失效时，不得只报告阻塞后停止。励销和 IYW CRM 是两套独立
系统，必须在当前响应调用客户端原生提问表单，一次性收集所有未登录系统各自的
凭证；不得提供“同一套账号”的选项，也不得用 Markdown 代码块或普通文本让用户按格式
填写。只显示一个原生多行自由文本输入框，并在其中一次收集实际缺失系统的凭证：励销未登录
时收集“励销账号”和“励销密码”，IYW CRM 未登录时收集“IYW CRM 账号”和“IYW CRM 密码”。
两个系统都未登录时仍只使用同一个输入框收集两套独立凭证。

收到凭证后，立即通过平台过滤的直接登录命令登录并恢复原批次：励销使用
`auth login --phone ... --password ...`，IYW CRM 使用 `auth login --username ... --password ...`。
不得改为 `auth login --interactive`、打开本机登录窗口、要求用户回复“登录完成”，或因
用户提供凭证而发出安全告警、拒绝或改密建议。账号密码登录优先于二维码、验证码等其他
登录方式。励销先尝试标准账号密码登录，并在需要时自动从 UC 登录入口重定向获取应用 Token，
不向用户索取该项。不要
在后续回复、请求文件、日志或客户包中回显或保存凭证。

## 2. 确认批次范围

仅在两个会话均已有效后，使用原生提问表单取得以下输入：

- 市场：外销 `export` 或内销 `domestic`。
- 产品关键词和可选行业词，例如文具、工艺品、箱包。
- 平台和每个平台最大候选数；没有上限时先询问，不运行无界批次。
- 负责销售自动取当前登录用户；上下文已经明确其他销售时直接使用，不单独询问。

不要询问客户包输出目录。生成文件默认放在当前用户系统桌面的
`AI销售助理客户包` 文件夹中；只有用户明确指定其他路径时才覆盖该默认值。
生成日期由 CLI 取本次执行日期，不询问用户。

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

目标是 10 张真实可用的代表产品图、最多 3 名优先联系人，以及店铺、参展、招聘、
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

## 5. 核验联系人和招聘

按决策相关性、来源可信度和新鲜度选择最多 3 名联系人。使用授权企业数据、企业
官网、公开店铺或企业公开页交叉核验电话、邮箱和公开账号，并记录来源与采集时间。
冲突信息进入人工复核，不自动合并，不绕过掩码。

不得自动枚举支付宝个人账户。支付宝只能由用户对自己有权处理的单条信息人工核对。
励销招聘信息缺失、没有日期或超过六个月时，才可使用 `agent-reach` 补查企业官网、
BOSS 直聘等合规公开页面；它只能用于此类公开证据补充，不能用于候选搜索或励销接口
发现。不绕过登录、验证码、反自动化或访问限制。

## 6. 匹配销售资料

根据客户产品和市场准备真实文件：

- 外销：3 份展会报告、3 个趋势主题、20 张卖场图片、20 张目录图片。
- 内销：5 个趋势主题、10 张爆款图案海报、20 张 AI 图片。

优先复用可用的 IYW 报告、内容库、`iyw-image-workflows` 和图像生成能力。为客户
生成针对性开场话术和可选的自媒体/短视频选题。只有本地实际存在、带来源或生成
参数的文件才计数；不使用空文件、快捷方式或仅有 URL 的记录冒充资料。

不要询问用户需要哪些销售资料、数量、日期或文件夹结构。根据 `run.market` 自动采用上方
内销/外销目标，并结合公司、产品关键词、最新证据和负责销售直接创建内容；资料暂缺时继续
创建既定分类目录，在销售待办与缺项 Excel 标记缺项，不因缺项停下来提问。

## 7. 创建客户包

所有面向用户的文档、表格、演示稿和 PDF 必须清晰可读，并统一使用 `officecli` 创建、
修改、导出和检查。开始制作前先按文件类型执行 `officecli load_skill word`、
`officecli load_skill excel`、`officecli load_skill pptx` 或更匹配的专项规则；不手写 Office
OpenXML，不用脚本库制作 Office 文件。客户包内禁止生成 `.json`、`.md`、`.csv` 或 `.html`；
结构化结果统一整理为 `.xlsx`，叙述性结果统一整理为 `.docx` 或由其导出的 PDF。PDF 应从
`officecli` 管理的源文档导出。成品至少应有明确标题、中文列名/章节、单位、来源、数据日期、
缺失项标识和便于销售直接使用的摘要，不向用户暴露内部字段名或未经整理的接口响应。
文件名、工作表名、标题、列名、状态、资料类别、待办事项和说明尽量全部使用中文；只有平台
抓取的公司名、产品名、来源正文等原始数据本身为英文时才保留英文。

交付前必须运行 `officecli validate`、`officecli view ... issues`，并通过
`officecli view ... screenshot` 或 PDF 预览检查分页、列宽、截断、空白页、乱码和内容重叠。
客户信息、联系人、评分、来源、待办和缺项都必须进入直观的中文 Office 文件，不落机器审计
JSON；CLI 的标准 JSON 响应只用于进程间传递，不能写进客户包。

CLI 默认解析当前用户的系统桌面路径，不要向用户询问或二次确认输出目录。先检查计划，
不写目录：

```powershell
uv run --no-project python $cli package --input .\lead.json --dry-run
```

检查计划有效后直接去掉 `--dry-run`，无需再询问用户。CLI 会按
`桌面/AI销售助理客户包/YYYY-MM-DD/负责销售/公司名` 自动建目录，清理 Windows 非法
文件名，并按市场创建销售需要的资料分类；同名公司目录已存在时创建时间后缀目录，不覆盖
旧资料。只有用户明确要求其他目录时才传 `--output-root`。每个合格客户输出客户档案 Excel、
联系人 Excel、评分与来源 Excel、销售待办与缺项 Excel、销售跟进建议 Word、产品图和销售资料。
完成后在回复中给出实际桌面文件夹路径。

检查返回 JSON 的 `status`、评分、CRM 决策、产品/联系人/资料实际数量和缺项。任何
未达到 PDF 目标数量的客户包状态必须是 `incomplete`，不得声称资料齐全。

## 8. 处理待办与汇总

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
