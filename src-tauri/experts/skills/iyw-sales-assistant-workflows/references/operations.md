# Sales Assistant Operations

## Platform Routing

| 市场 | 平台 | 主要目标 |
| --- | --- | --- |
| 外销 | 阿里国际站、中国制造网、环球资源 | 企业、产品、联系人、参展与店铺信息 |
| 外销 | Amazon、SHEIN | 产品、品牌、店铺更新和市场匹配信号 |
| 内销 | 1688、天猫、京东 | 企业、产品、品牌、店铺更新和联系方式 |

先按产品搜索，再根据企业名称补查。每个平台必须设置候选上限。不同平台返回同一
统一社会信用代码时合并；没有代码时使用规范化公司全称，名称近似但证据不足时不
自动合并。

## Lixiao Sequence

1. `auth ensure`。
2. `feature-packages` 和 `permission-info`。
3. `scene-search` 或 `scene-search-products`。
4. `company-card`、`company-products`、`company-exhibitions`。
5. `company-management`、`company-recruitment`、`company-ip`、`company-brand`。
6. `company-contacts-count` 和权限允许的联系人查询。

只在产品或联系人确实隐藏、额度可用且用户明确授权当前公司时传
`--unlock-if-needed`。解锁响应成功不等于详情已经可见；检查最终 `view_available`
和 `contacts_after_unlock`。不得遍历搜索结果批量解锁。

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

另选 10 张代表产品图和最多 3 名联系人。只统计真实存在的唯一文件路径。AI 文件要
保留生成参数或任务来源，报告和趋势要保留内容库来源。缺项必须出现在 manifest，
状态使用 `incomplete`。

## Package Layout

```text
<output-root>/
└── <公司名>/
    ├── manifest.json
    ├── 01-客户档案/
    ├── 02-产品图片/
    ├── 03-联系人/
    ├── 04-匹配资料/
    ├── 05-销售话术/
    └── 06-待办/pending-actions.json
```

Windows 非法字符替换为下划线。禁止覆盖同名目录。CRM 保护、已归属或待复核客户
不写客户包。

## Side-Effect Gates

| 动作 | 默认 | 放行条件 |
| --- | --- | --- |
| 励销企业解锁 | 禁止 | 具体公司明确授权 |
| 本地客户包 | dry-run | 用户确认输出目录 |
| CRM 捞取/新建 | `pending` | 捕获接口、确认、读回验证 |
| CRM 归属分配 | `pending` | 捕获接口、确认、读回验证 |
| 通知销售 | `pending` | 明确渠道、收件人、确认 |
| CRM 回写 | `pending` | 字段差异、确认、读回验证 |

待办文件是操作队列，不是成功凭证。没有权威写接口时保持 `pending`。
