# Sales Lead Data Contract

## Input

CLI 输入必须是一个 JSON 对象。拒绝未知顶层字段。所有时间使用带时区的 ISO 8601
格式，例如 `2026-07-29T12:00:00+08:00`。

必填顶层字段：

- `company`：对象，`name` 为非空公司全称。
- `run`：对象，必须含 `market`、`salesperson`、`as_of`。
- `activities`：活动证据数组。
- `crm`：CRM 匹配结果对象。

允许的可选顶层字段：`products`、`contacts`、`materials`、`outreach`、
`pending_actions`、`errors`。

### Enums

- `run.market`：`export` 或 `domestic`。
- `activities[].type`：`sales_hiring`、`designer_hiring`、`exhibition`、
  `shop_update`、`copyright_work`。
- `crm.match_status`：`matched`、`not_found`、`ambiguous`、`failed`。
- `materials[].type`：外销使用 `exhibition_report`、`trend_theme`、
  `retail_image`、`catalog_image`；内销使用 `trend_theme`、`pattern_poster`、
  `ai_image`。

### Sanitized Example

```json
{
  "company": {
    "name": "示例文具有限公司",
    "aliases": ["示例文具"],
    "lixiao_id": "example-id",
    "platform": "阿里国际站",
    "shop_url": "https://example.invalid/shop"
  },
  "run": {
    "market": "export",
    "salesperson": "销售甲",
    "as_of": "2026-07-29T12:00:00+08:00",
    "run_id": "20260729-stationery-001",
    "product_keywords": ["文具", "笔袋"]
  },
  "activities": [
    {
      "type": "designer_hiring",
      "observed_at": "2026-06-18T09:00:00+08:00",
      "source": "lixiao:company-recruitment",
      "evidence": "招聘平面设计师"
    },
    {
      "type": "exhibition",
      "observed_at": "2026-05-02T09:00:00+08:00",
      "source": "lixiao:company-exhibitions",
      "evidence": "2026 春季展会"
    }
  ],
  "crm": {
    "match_status": "matched",
    "record_id": "crm-example-id",
    "star": 2,
    "star_name": "2星",
    "owner": ""
  },
  "products": [
    {
      "name": "帆布笔袋",
      "local_path": "C:\\staging\\products\\pencil-case.jpg",
      "source": "lixiao:company-products",
      "representative": true
    }
  ],
  "contacts": [
    {
      "name": "业务负责人",
      "role": "外贸经理",
      "phone": "公开企业电话",
      "source": "company_public_page",
      "observed_at": "2026-07-29T10:00:00+08:00",
      "verification": "confirmed_by_two_public_sources"
    }
  ],
  "materials": [
    {
      "type": "trend_theme",
      "local_path": "C:\\staging\\materials\\trend-01.pdf",
      "source": "iyw-content-library"
    }
  ],
  "outreach": {
    "opening_copy": "您好，我们结合贵司笔袋产品准备了近期外销趋势资料。",
    "social_ideas": ["文具消费趋势短视频", "笔袋材质卖点图文"]
  },
  "pending_actions": [],
  "errors": []
}
```

不要把密码、Cookie、Token、验证码、请求头或完整上游原始响应放入输入 JSON。

## Evaluation Output

`evaluate` 返回：

- `score.total`：0 至 50。
- `score.breakdown`：五类活动的 0/10 分值。
- `score.accepted`：实际计分证据。
- `crm_decision.decision`：CRM 决策代码。
- `crm_decision.eligible`：是否可进入客户包流程。

## Package Output

`package` 返回：

- `package_dir`：计划或实际客户目录。
- `status`：`complete`、`incomplete`、`skipped` 或 `review`。
- `products`、`contacts`、`materials`：目标、实际、缺项和选中记录。
- `pending_actions`：仍需确认并接入接口的外部动作。
- `network`、`crm_write`：固定为 `false`。
- `writes`：本次是否写入了本地客户包。

标准响应为 `{"ok": true, "data": ...}`。校验或文件错误返回
`{"ok": false, "error": {"code": ..., "message": ..., "retryable": false}}`。

只有本地路径指向真实文件时，产品或资料才计入实际数量。相同解析路径只计一次。
CLI 不覆盖已有公司目录。标准 JSON 响应仅用于调用方读取执行状态，不写入客户目录。

CLI 使用执行日期和 `run.salesperson` 自动生成
`<output-root>/<YYYY-MM-DD>/<负责销售>/<公司名>`。`run.market` 决定销售资料目标和
`05-销售资料` 下的分类目录；缺少资料时仍创建分类目录，并在
`07-待办/销售待办与缺项.xlsx` 中直观列出目标数量、实际数量和缺少数量，不要求用户选择
日期、目录或资料种类。客户包内不生成 `.json`、`.md`、`.csv` 或 `.html` 文件；客户、
联系人、评分、来源、待办和缺项使用 `.xlsx`，销售建议使用 `.docx`。
