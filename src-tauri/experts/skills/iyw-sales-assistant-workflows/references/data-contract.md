# Sales Lead Data Contract

## Input

CLI 输入必须是一个 JSON 对象。拒绝未知顶层字段。所有时间使用带时区的 ISO 8601
格式，例如 `2026-07-29T12:00:00+08:00`。

必填顶层字段：

- `company`：对象，`name` 为非空公司全称。
- `run`：对象，必须含 `market`、`salesperson`、`as_of`。
- `activities`：活动证据数组。
- `crm`：CRM 匹配结果对象。

允许的可选顶层字段：`products`、`contacts`、`materials`、`material_workflow`、
`track_results`、`outreach`、`pending_actions`、`errors`。

`company.business_info` 可选字段包括 `unified_social_credit_code`、`legal_representative`、
`registration_number`、`organization_code`、`registered_capital`、`paid_in_capital`、
`company_type`、`industry`、`approval_date`、`business_period` 和 `english_name`。

`run.market_keywords` 是可选的市场分组数组，优先使用 `欧美`、`中东`、`日韩`、`国内`，
也可追加产品相关关键词。`products[].product_url` 和 `products[].store_url` 是可选的
商品及店铺链接，必须使用 HTTPS。

`outreach.opening_copy` 保存针对性开场白。`outreach.ppt_path` 保存 PPT 子代理通过
`presentations` Skill 和 artifact-tool 生成并完成渲染检查的真实本地 `.pptx` 路径；
`outreach.ppt_manifest` 保存公司键、七个必需章节，以及逐页渲染数、重叠和文字适配 QA 回执。
批次校验清单、至少六页、公司名正文后再复制，并重新执行渲染和越界检查。字段缺失、包结构
无效、内容清单不完整或二次 QA 失败时，批次使用
内置 artifact-tool 生成器尝试补生成；生成环境不可用时只把该公司标记为 PPT 缺项，不阻断其他公司。

`material_workflow.attempts[]` 记录图片工作流首选尝试。每项至少包含
`provider: "iyw-image-workflows"`、资料 `type`、与该类型匹配的固定 `alias` 和最终
`status`。首选和补充资料都必须有对应尝试；首选资料还要求来源 alias 的状态为成功或部分成功。
`track_results[]` 记录宿主 Agent 的四条公司轨道，字段为 `company_key`、`track`、
`status`、`missing` 和可选 `error`；四种轨道各有且仅有一条，缺失、重复或任何非完成状态都会
使该公司保持 `incomplete`。
`ai_image` 还必须包含 `generation_receipt.status: "succeeded"` 和至少一个 task ID；排队中、
运行中、失败或缺少任务凭证的生成结果不计数。

### Enums

- `run.market`：`export` 或 `domestic`。
- `activities[].type`：评分使用 `sales_hiring`、`designer_hiring`、`exhibition`、
  `shop_update`、`copyright_work`；展示招聘/知识产权/参展明细时还可使用
  `recruitment`、`trademark`、`patent`。新增类型不改变原有五类六个月评分规则。
- `crm.match_status`：`matched`、`not_found`、`ambiguous`、`failed`。
- `materials[].type`：外销使用 `exhibition_report`、`trend_theme`、
  `retail_image`、`catalog_image`；内销使用 `trend_theme`、`pattern_poster`、
  `ai_image`。

### Product Images And Analysis

- `products[].image_url`：励销返回的 HTTPS 产品图片地址，下载阶段使用，不写入最终交付。
- `products[].local_path`：`download-products` 生成的真实本地图片路径；文件必须可完整解析。
- `products[].source`：固定为 `lixiao:company-products`。
- `products[].company_id`/`company_name`/`store_url`：至少一个必须与当前公司正向匹配。
- `products[].download_receipt`：原始 URL 必须等于 `image_url`，并记录最终 HTTPS URL 和匹配文件的 SHA-256。
- `products[].analysis`：图片分析子代理返回的短结论对象。允许字段为 `summary`、
  `selling_points`、`target_market`、`sales_angle`；不要存评分、采集过程或推理过程。

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
      "image_url": "https://example.invalid/product/pencil-case.jpg",
      "local_path": "C:\\staging\\products\\pencil-case.jpg",
      "source": "lixiao:company-products",
      "representative": true,
      "analysis": {
        "summary": "帆布材质与大面积图案适合文创和礼赠渠道。",
        "selling_points": ["可做系列图案", "适合小批量联名"],
        "target_market": "文创零售与企业礼赠",
        "sales_angle": "从节日礼赠和联名图案开发切入。"
      }
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
      "source": "iyw-image-workflows:trend-detail"
    }
  ],
  "material_workflow": {
    "attempts": [
      {
        "provider": "iyw-image-workflows",
        "type": "trend_theme",
        "alias": "trend-detail",
        "status": "partial"
      }
    ]
  },
  "track_results": [
    {
      "company_key": "example-id",
      "track": "business_contacts",
      "status": "completed",
      "missing": []
    },
    {
      "company_key": "example-id",
      "track": "product_images",
      "status": "completed",
      "missing": []
    },
    {
      "company_key": "example-id",
      "track": "activity_evidence",
      "status": "completed",
      "missing": []
    },
    {
      "company_key": "example-id",
      "track": "image_materials_ppt",
      "status": "completed",
      "missing": []
    }
  ],
  "outreach": {
    "opening_copy": "您好，我们结合贵司笔袋产品准备了近期外销趋势资料。",
    "social_ideas": ["文具消费趋势短视频", "笔袋材质卖点图文"],
    "ppt_path": "C:/temp/example-id/示例文具有限公司-销售资料.pptx",
    "ppt_manifest": {
      "company_key": "example-id",
      "sections": ["business_info", "product_images", "market_keywords", "store_links", "activities", "materials", "opening_copy"],
      "qa": {
        "status": "completed",
        "rendered_slides": 6,
        "overlap_checked": true,
        "text_fit_checked": true
      }
    }
  },
  "pending_actions": [],
  "errors": []
}
```

不要把密码、Cookie、Token、验证码、请求头或完整上游原始响应放入输入 JSON。

## Product Download Output

`download-products` 接收一条完整公司记录，将 `products[].image_url` 下载到指定临时目录，
返回更新后的 `products`、`saved_paths`、逐张脱敏 `errors`、`network: true` 和
`crm_write: false`。主代理必须将返回的本地路径交给图片分析子代理，再把分析结论写回
`products[].analysis`。单张失败不终止其他图片。

## Evaluation Output

`evaluate` 返回：

- `score.total`：0 至 50。
- `score.breakdown`：五类活动的 0/10 分值。
- `score.accepted`：实际计分证据。
- `crm_decision.decision`：CRM 决策代码。
- `crm_decision.eligible`：是否可进入客户包流程。

## Batch Input And Output

`batch-package` 输入为：

```json
{
  "records": [
    {"company": {}, "run": {}, "activities": [], "crm": {}}
  ],
  "run": {"as_of": "2026-08-03T12:00:00+08:00"}
}
```

`records` 必须是非空数组，每个元素使用本文单家公司契约。根级 `run` 可选，仅供上层
记录批次上下文。`batch-package` 返回：

- `batch_dir`：实际日期目录；默认 `<桌面>/<YYYY-MM-DD>`。
- `workbook`：唯一汇总文件 `<batch_dir>/今日推荐公司.xlsx`。
- `companies`：各公司推荐状态和资产目录，不包含原始采集响应。
- `summary`：`recommended`、`complete`、`incomplete`、`skipped`、`review`、`failed` 数量。
- `network`、`crm_write`：固定为 `false`。
- `writes`：本次是否写入批次目录。

最终目录结构：

```text
<桌面>/<YYYY-MM-DD>/今日推荐公司.xlsx
<桌面>/<YYYY-MM-DD>/<公司名>/产品图片/
<桌面>/<YYYY-MM-DD>/<公司名>/销售资料/
<桌面>/<YYYY-MM-DD>/<公司名>/准备资料/<公司名>-销售资料.pptx
```

工作簿只有一个“今日推荐公司”工作表，固定十列：`公司名`、`前三联系及角色`、
`前三联系人电话`、`基本信息(工商信息)`、`产品图`、`市场关键词`、`店铺`、
`近半年招聘/知识产权/参展情况`、`准备资料`、`针对性开场白`。它不展示采集过程、评分过程、原始来源、内部 CRM
决策或错误详情。日期目录已存在时使用 `YYYY-MM-DD-HHMMSS`，不覆盖旧批次。

## Legacy Single-Company Package Output

`package` 返回：

- `package_dir`：计划或实际客户目录。
- `status`：`complete`、`incomplete`、`skipped` 或 `review`。
- `products`、`contacts`、`materials`：目标、实际、缺项和选中记录。
- `pending_actions`：仍需确认并接入接口的外部动作。
- `network`、`crm_write`：固定为 `false`。
- `writes`：本次是否写入了本地客户包。

标准响应为 `{"ok": true, "data": ...}`。校验或文件错误返回
`{"ok": false, "error": {"code": ..., "message": ..., "retryable": false}}`。

只有本地路径指向真实非空文件时，产品或资料才计入实际数量。图片必须通过完整容器解析，
报告类资料必须使用受支持的 Office/PDF 后缀；相同解析路径只计一次。
CLI 不覆盖已有公司目录。标准 JSON 响应仅用于调用方读取执行状态，不写入客户目录。

兼容命令 `package` 仍使用执行日期和 `run.salesperson` 自动生成
`<output-root>/<YYYY-MM-DD>/<负责销售>/<公司名>`。`run.market` 决定销售资料目标和
`05-销售资料` 下的分类目录；缺少资料时仍创建分类目录，并在
`07-待办/销售待办与缺项.xlsx` 中直观列出目标数量、实际数量和缺少数量，不要求用户选择
日期、目录或资料种类。客户包内不生成 `.json`、`.md`、`.csv` 或 `.html` 文件；客户、
联系人、评分、来源、待办和缺项使用 `.xlsx`，销售建议使用 `.docx`。默认销售助理流程
不再调用该兼容命令，必须使用 `batch-package` 生成批次级单工作表交付。
