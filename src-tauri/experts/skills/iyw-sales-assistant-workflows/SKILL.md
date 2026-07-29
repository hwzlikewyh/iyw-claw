---
name: iyw-sales-assistant-workflows
description: 编排励销与爱原物 CRM 完成 AI 销售助理客户开发，包括外销/内销平台获客、企业与联系人采集、CRM 查重和 4/5/10 星保护、最近六个月线索评分、招聘与联系方式核验、针对性趋势及 AI 资料匹配、销售开场话术和桌面客户包生成。用户提到 AI 销售助理、励销获客、CRM 客户归属、销售线索评分、阿里国际站/1688 等平台找客户、销售产品包或客户资料回传时使用。
---

# IYW AI Sales Assistant Workflows

复用 `lixiao-workflows` 和 `iyw-crm-workflows` 的认证与已捕获接口。使用内置 CLI
确定性计算评分、执行 CRM 门禁、核对资料数量并创建客户包。不要在本 Skill 中猜测
新接口或重复保存凭据。

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

## 1. 确认批次范围

开始前取得以下输入：

- 市场：外销 `export` 或内销 `domestic`。
- 产品关键词和可选行业词，例如文具、工艺品、箱包。
- 平台和每个平台最大候选数；没有上限时先询问，不运行无界批次。
- 负责销售和客户包输出根目录。
- 是否允许解锁某一家具体企业；不得把授权扩展到其他公司。

外销平台包括阿里国际站、中国制造网、环球资源、Amazon、SHEIN。内销平台包括
1688、天猫、京东。

## 2. 验证会话与权限

1. 按 `lixiao-workflows` 执行 `auth status` 和 `auth ensure`。
2. 按 `iyw-crm-workflows` 执行 `auth status` 和 `auth ensure`。
3. CRM 真实请求使用 HTTP 时，只有用户明确允许后才传
   `--allow-insecure-http`。
4. 不在对话、请求文件或客户包中放入密码、Cookie、Token 或验证码证明。

认证失效时让用户在本地终端执行相应的交互登录。不要索取或复用聊天中的密码。

## 3. 搜索并采集企业

通过 `lixiao-workflows` 执行：

1. `feature-packages` 和 `permission-info` 检查可用权限。
2. `scene-search` 或 `scene-search-products` 按平台和产品搜索。
3. 用统一社会信用代码优先、公司全称其次进行批次内去重。
4. 对候选调用 `company-card`、`company-products`、`company-exhibitions`、
   `company-management`、`company-recruitment`、`company-ip`、`company-brand`、
   `company-contacts-count` 和允许的联系人操作。

目标是 10 张真实可用的代表产品图、最多 3 名优先联系人，以及店铺、参展、招聘、
版权和品牌证据。隐藏详情只记录缺口。只有用户授权该公司时才可执行
`company-products --unlock-if-needed`；禁止批量解锁。

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
励销招聘信息缺失、没有日期或超过六个月时，才补查企业官网、BOSS 直聘等合规
公开页面；不绕过登录、验证码、反自动化或访问限制。

## 6. 匹配销售资料

根据客户产品和市场准备真实文件：

- 外销：3 份展会报告、3 个趋势主题、20 张卖场图片、20 张目录图片。
- 内销：5 个趋势主题、10 张爆款图案海报、20 张 AI 图片。

优先复用可用的 IYW 报告、内容库、`iyw-image-workflows` 和图像生成能力。为客户
生成针对性开场话术和可选的自媒体/短视频选题。只有本地实际存在、带来源或生成
参数的文件才计数；不使用空文件、快捷方式或仅有 URL 的记录冒充资料。

## 7. 创建客户包

先检查计划，不写目录：

```powershell
uv run --no-project python $cli package --input .\lead.json `
  --output-root "$HOME\Desktop\AI销售助理客户包" --dry-run
```

确认输出目录后去掉 `--dry-run`。CLI 会清理 Windows 非法文件名，同名公司目录已
存在时创建时间后缀目录，不覆盖旧资料。每个合格客户输出客户档案、来源、产品图、
联系人、匹配资料、话术、`manifest.json` 和 `pending-actions.json`。

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
