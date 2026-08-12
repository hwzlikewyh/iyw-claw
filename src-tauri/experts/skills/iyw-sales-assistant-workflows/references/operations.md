# Sales Assistant Operations

## Platform Routing

| 市场 | 平台 | 主要目标 |
| --- | --- | --- |
| 外销 | 阿里国际站、中国制造网、环球资源 | 企业、产品、联系人、参展与店铺信息 |
| 外销 | Amazon、SHEIN | 产品、品牌、店铺更新和市场匹配信号 |
| 内销 | 1688、天猫、京东 | 企业、产品、品牌、店铺更新和联系方式 |

候选只能来自励销业务级搜索命令。禁止 Agent 使用 curl、`agent-reach`、网页搜索或
逐个 `api` 子命令采集候选。先按产品搜索，再根据企业名称补查。每个平台必须设置候选
上限。不同平台返回同一统一社会信用代码时合并；没有代码时使用规范化公司全称，名称
近似但证据不足时不自动合并。

## Lixiao Sequence

1. `auth ensure`。
2. Agent 调用一次 `workflow ecommerce-search`，传平台、关键词和每平台上限。
3. 命令内部动态读取 `searchConditionConfig`、解析平台筛选项并自动分页调用场景搜索。
4. Agent 完成 CRM 门禁后，将合格企业 ID 一次传给 `workflow company-profile`。
5. 档案命令内部完成详情、产品按需解锁和联系人采集。

```powershell
uv run --no-project python $lixiaoCli workflow ecommerce-search `
  --keyword 毛绒玩具 --platform 1688 --platform 天猫 --platform 京东 `
  --limit-per-platform 100
uv run --no-project python $lixiaoCli workflow company-profile `
  --id <company-id-1> --id <company-id-2>
```

真实搜索每次读取最新筛选配置，动态解析平台编号、商品名称字段、匹配运算符和输入约束，
不使用内置编号代替服务端配置。只有 `--dry-run` 在不访问网络时使用已捕获映射生成计划。
契约缺失或平台不受支持时快速失败，不允许 Agent 反向抓包。

只在产品或联系人确实隐藏且额度可用时，为当前公司直接传
`--unlock-if-needed`；该参数即为该企业的解锁授权，无需向用户二次提问。解锁响应成功不等于
详情已经可见；检查最终 `view_available` 和 `contacts_after_unlock`。不得遍历搜索结果批量解锁。

## Six-Month Score

以 `run.as_of` 为结束时间，向前减六个日历月并包含边界。每类只要有一条带来源、
日期在窗口内的证据就计 10 分，多条不累加：

| 类型 | 业务含义 | 分值 |
| --- | --- | ---: |
| `sales_hiring` | 招聘销售人员 | 10 |
| `designer_hiring` | 招聘设计师 | 10 |
| `exhibition` | 参加展会 | 10 |
| `shop_update` | 店铺或商品更新 | 10 |
| `copyright_work` | 发布或登记版权作品 | 10 |

缺少来源、缺少日期、日期不可解析、早于窗口或晚于基准日均不计分。总分上限 50。
排序依次使用总分、证据完整度、最近活动时间。

## CRM Gate

| 条件 | 决策 | 可进入客户包 |
| --- | --- | --- |
| CRM 查询失败 | `crm_unverified` | 否 |
| 多记录无法唯一匹配 | `crm_ambiguous` | 否 |
| 星级不可解析 | `crm_review` | 否 |
| 4、5、10 星 | `skip_protected_star` | 否 |
| 有归属人 | `skip_owned` | 否 |
| 唯一记录无归属 | `eligible_unowned` | 是 |
| CRM 未找到 | `eligible_new` | 是 |

优先匹配公司全称。别名只能用于补查，不能让弱相似结果覆盖全称结果。CRM 失败时
不得把客户当成“未找到”。

## Contacts And Recruitment

优先联系人顺序：业务负责人、外贸/销售负责人、设计或产品负责人。最多保留 3 名，
且每名至少有一个公开电话、邮箱或公开账号以及来源。相同联系方式不重复计数。

可信来源优先级：授权企业数据、企业官网、平台企业公开页、公开店铺、其他可审计
公开来源。保存来源和观察时间。冲突信息并列标记，不猜测哪个为真。

不自动枚举支付宝账户，不绕过联系方式掩码。励销招聘证据缺失、无日期或超过六个
月时，才使用企业官网或 BOSS 等公开招聘页面补充；遵守登录和访问限制。

## Material Targets

外销：

- `exhibition_report`：3。
- `trend_theme`：3。
- `retail_image`：20。
- `catalog_image`：20。

内销：

- `trend_theme`：5。
- `pattern_poster`：10。
- `ai_image`：20。

`iyw-image-workflows` 是以上六类资料的首选来源。每家公司必须先按该 Skill 当前入口设置
`$searchCli` 和 `$commerceCli`，调用前读取其 SKILL 及相关 reference；销售 Skill 不自行
构造接口或复制 payload 契约。

| 资料类型 | 固定能力或别名 |
| --- | --- |
| `exhibition_report` | `report-list`、`report-detail`、`report-full`、`report-images` |
| `trend_theme` | `trend-list`、`trend-detail` |
| `retail_image` | `image` |
| `catalog_image` | `catalog` |
| `pattern_poster` | `ip-patterns` |
| `ai_image` | `$commerceCli fission-generate` |

搜索别名返回元数据或远程 HTTPS 资源时，先把资源下载或保存到暂存目录，并在每个
`materials[]` 项中写入实际 `source`；只有真实本地文件计入 `actual`。图片 Skill 返回的
真实文件仍低于目标时，才可使用合规公开检索、其他图库或其他生图能力补足。该补足规则仅
适用于销售资料，不改变候选搜索、联系人和招聘补查限制。主代理必须将每类调用写入
`material_workflow.attempts[]`；`batch-package` 校验 provider、type、固定 alias 和最终 status。
首选文件和补充文件都必须有类型匹配的尝试；首选文件的来源 alias 还必须对应成功或部分成功
状态。`ai_image` 还必须带状态为 `succeeded` 且包含
task ID 的 `generation_receipt`。完成补足后仍不足时，在对应销售字段标记缺项并继续。

另选 10 张代表产品图和最多 3 名联系人。产品图片先用 `download-products` 从 HTTPS 地址
下载并完整解析，再将真实本地路径交给图片分析子代理。产品 source 必须是
`lixiao:company-products`，至少一个公司 ID、公司名或店铺域名必须正向匹配；receipt 原始 URL
必须等于 `image_url`，最终 HTTPS URL 和 SHA-256 必须有效。只统计真实存在且可解析的唯一文件路径。AI 文件要
保留生成参数或任务来源；来自首选来源的报告、趋势、图片、画册和图案要保留
`iyw-image-workflows:<能力>` 来源，补足资料保留其实际来源。缺项在现有销售字段中简要标记，
内部状态使用 `incomplete`。

文件生成不向用户询问日期、销售、资料类型、数量或目录结构。日期取执行日期；销售取
当前登录用户或上下文已明确的负责人；资料目标由市场自动选择。即使资料缺失，也创建完整
分类目录并在批次状态中记录缺口，继续处理其他内容。

## Parallel Company Tracks

CRM 门禁通过后，按公司使用有界并发分派四条独立子代理轨道；每家公司最多并行 4 条轨道，
批次全局最多并行 8 条轨道：

1. `工商与联系人`：工商字段、前三联系人及角色、电话、店铺和商品链接。
2. `产品图片`：店铺产品图片下载、真实文件校验和短分析。
3. `活动证据`：招聘、知识产权、参展；先查六个月，某一类为空时只把该类扩展到一年，
   并标记“近一年补充”。
4. `图片工作流资料与 PPT`：优先使用 `iyw-image-workflows` 完成资料和来源清单，生成
   针对性开场白，并通过 artifact-tool 为每家公司生成一份 PPT。

每条轨道只能写自己的私有暂存目录或返回结构化结果。子代理不得写共享工作簿、最终 PPT 或
其他公司的目录。轨道统一返回 `company_key`、`track`、`status`、`missing`、可选 `error`
和 `result`。主代理等待全部结果，把异常写入 `track_results[]`；四种轨道每家公司各一条，
缺失、重复或非完成状态均不得标记 `complete`。再校验真实本地路径、资料来源
和公司归属后合并；
**主代理独占最终 Excel 和 PPT 写入**，并且只有主代理调用 `batch-package`。单条轨道失败只记录该公司缺项，不阻断
其他公司。上游 PPT 轨道优先提供 `outreach.ppt_path` 和 `outreach.ppt_manifest`；清单记录公司键、
七个必需章节、逐页渲染数、重叠检查和文字适配检查。批次校验内容和清单后重新执行渲染与越界
检查，缺失或 QA 失败时由内置生成器尝试补生成。

## Package Layout

最终批次 Excel 必须使用 `officecli` 制作。先加载 `excel` 规则，完成后执行结构校验、问题
检查和渲染预览。表格须有中文列名、筛选友好的结构、可读列宽和不重叠的产品缩略图。
批次目录内不得出现 `.json`、`.md`、`.csv`、`.html`、逐家公司 Excel 或 Word；CLI 的 JSON
响应只在进程间使用，不写入最终目录。最终表不展示采集过程、评分过程、原始来源或内部
CRM 决策，只保留销售可直接使用的公司、产品、联系人、切入点、目录链接和缺项状态。

```text
<用户系统桌面>/
└── <YYYY-MM-DD>/
    ├── 今日推荐公司.xlsx
    └── <公司名>/
        ├── 产品图片/
        ├── 销售资料/<市场目标分类>/
        └── 准备资料/<公司名>-销售资料.pptx
```

工作簿固定十列且每家公司一行：`公司名`、`前三联系及角色`、`前三联系人电话`、
`基本信息(工商信息)`、`产品图`、`市场关键词`、`店铺`、
`近半年招聘/知识产权/参展情况`、`准备资料`、`针对性开场白`。`产品图` 列内放一张锚定到
该单元格的店铺代表产品缩略图，并链接到完整产品图片目录；`准备资料` 链接到公司 PPT。长文本
保留在单元格内、自动换行，并仅按 Excel 可读上限限制行高。不得恢复旧的产品分析、状态或三张独立预览列。

Windows 非法字符替换为下划线。禁止覆盖同名日期目录；重复运行添加时间后缀。CRM 保护、
已归属或待复核客户不写推荐行和公司目录。默认流程不得询问或二次确认输出目录；只有用户
明确指定其他路径时才传 `--output-root`。完成后向用户提供实际日期目录。

## Side-Effect Gates

| 动作 | 默认 | 放行条件 |
| --- | --- | --- |
| 励销企业解锁 | 按需执行 | 当前公司详情隐藏且传入 `--unlock-if-needed` |
| 本地批次交付 | dry-run 后执行 | 默认写入系统桌面日期目录，无需二次确认 |
| CRM 捞取/新建 | `pending` | 捕获接口、确认、读回验证 |
| CRM 归属分配 | `pending` | 捕获接口、确认、读回验证 |
| 通知销售 | `pending` | 明确渠道、收件人、确认 |
| CRM 回写 | `pending` | 字段差异、确认、读回验证 |

待办文件是操作队列，不是成功凭证。没有权威写接口时保持 `pending`。
