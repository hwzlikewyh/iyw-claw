# 智配报告分页、分享与口令

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

统一 fetch_iyw_url。报告列表使用 page/page_size/search_text/status/is_read/is_shared；过滤值未选择则省略。reset 仅传 run_id，由服务生成新 share_password，替代旧表的自定义 sharePassword 摘要。分享口令可返回给授权查看的用户；不提供或读取额外登录 token。

```json
{"description":"查询未读智配报告","url":"https://gateway.iyw.cn/ai-agent-new/api/trend-push/runs/query","body":{"page":1,"page_size":10,"is_read":0}}
```

## 来源详细资料

## 七、趋势推送 / 智配报告（`ai-agent-new/api/trend-push`，8 个接口）

源码 `ai_4260.cf4494be.js`、`ai_5184.e502d3bd.js`、`ai_7965.e4742934.js`。基址固定 `https://gateway.iyw.cn`。

| 请求类型 | 接口 | 功能 | 入参 | 参数含义 / 返回 |
| --- | --- | --- | --- | --- |
| GET | `/ai-agent-new/api/trend-push/auto-report/setting` | 读取「每周自动生成智配报告」开关 | 无 | 返回 `data.enabled`（布尔）；加载失败时前端显示错误态 |
| POST | `/ai-agent-new/api/trend-push/auto-report/setting` | 保存自动报告开关 | `{ "enabled": true }` | 前端用 `Boolean(t)` 强制转布尔；返回 `data.enabled` |
| POST | `/ai-agent-new/api/trend-push/runs/query` | 报告列表（分页 + 筛选） | `page`、`page_size`、`search_text`、`status`、`is_read`、`is_shared` | `page` 默认 1、`page_size` 默认 10；`search_text` 关键词；`status` 为空表示全部；`is_read` / `is_shared` 传 `0`/`1` 才生效，空值不传；返回 `data.list[]`、`data.total` |
| GET | `/ai-agent-new/api/trend-push/runs/detail` | 报告详情 | Query：`run_id`、`pwd`（可选）；Headers：`token`（分享链接里带） | 分享出去的链接只有 `run_id` + `pwd` 两参数；返回 `data.pdfUrl` 等页面数据，前端用 `downloadPdf()` 暴露给外部 |
| POST | `/ai-agent-new/api/trend-push/runs/share` | 生成分享链接 | `{ "run_id": "..." }` | 返回 `data.share_password`；前端拼成 `/trendPushResult?run_id=&pwd=` 或 `/trendPushBrief?run_id=&pwd=`（按 `classify==4` 区分） |
| POST | `/ai-agent-new/api/trend-push/runs/share/reset` | 重置分享密码 | `{ "run_id": "..." }` | 返回新的 `data.share_password` |
| POST | `/ai-agent-new/api/trend-push/runs/share/disable` | 取消分享 | `{ "run_id": "..." }` | 成功后刷新列表 |
| POST | `/ai-agent-new/api/trend-push/runs/delete` | 删除报告 | `{ "run_id": "..." }` | 二次确认后调用；删除不可恢复 |
