# 工厂订单、物流与图案订单

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

完整 URL 使用 https://gateway.iyw.cn + 表中路径；全部 POST，用 fetch_iyw_url。先列表取真实订单/物流 ID，再读详情；确认生产、支付、补发、重发、取消属于业务变更。shipping 标注“发货/查物流”语义混合，纯物流查询使用明确的 logisticsInquiry，不把 shipping 当只读。

## 来源详细资料

### 9.1 工厂订单 `/factoryOrder/*`（14 个，全部 POST）

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/factoryOrder/page` | 订单列表 | 分页 + 状态/时间筛选 |
| `/factoryOrder/getByOrderId` | 订单详情 | `orderId` |
| `/factoryOrder/addTextureOrder` | 下纹理/材质单 | 产品与规格字段 |
| `/factoryOrder/update` | 修改订单 | `orderId` + 字段 |
| `/factoryOrder/updateAddress` | 改收货地址 | `orderId`、地址字段 |
| `/factoryOrder/updateSaleRemark` | 修改销售备注 | `orderId`、`saleRemark` |
| `/factoryOrder/productRemark` | 产品备注 | `orderId`、备注 |
| `/factoryOrder/cancelOrder` | 取消订单 | `orderId` |
| `/factoryOrder/confirmProduction` | 确认生产 | `orderId` |
| `/factoryOrder/shipping` | 发货 / 查物流 | `orderId` |
| `/factoryOrder/logisticsInquiry` | 物流查询 | `orderId` |
| `/factoryOrder/getRendering` | 取渲染图 | `orderId` |
| `/factoryOrder/orderPay` | **支付订单** | `orderId`；写操作，未执行 |

### 9.2 物流单 `/orderLogistic/*`（6 个，全部 POST）

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/orderLogistic/page` | 物流单列表 | 分页 |
| `/orderLogistic/getById` | 物流单详情 | `id` |
| `/orderLogistic/productRemark` | 产品备注 | `id`、`remark` |
| `/orderLogistic/reissue` | 补发 | `id` |
| `/orderLogistic/resend` | 重发 | `id` |
| `/orderLogistic/cancel` | 取消物流单 | `id` |

### 9.3 图案订单 `/patternOrder/*`（2 个，POST）

| 接口 | 功能 | 关键入参 |
| --- | --- | --- |
| `/patternOrder/orderList` | 图案订单列表 | 分页 |
| `/patternOrder/getSourceFile` | 取源文件 | `orderId` |
