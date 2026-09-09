# 组织成员、部门、角色与点数补充

来源：2026-09-10《爱原物接口文档·分层详版（补充篇）》；前端反解和原采集者实测标记，不表示本次已调用。只读取当前任务相关小节；其他领域见 [接口索引](iyw-api-index.md)，冲突和认证/签名边界见 [调用约定补充](iyw-api-access-contracts.md)。

基址 https://gateway.iyw.cn；统一 fetch_iyw_url，保留每行 GET/POST。GET 的 /del/{id} 和 updateAiPointsType 具有写副作用，不做自动重试。Path ID 需URL编码，query参数放query。menuList 和 getAiPointsType 的具体表确认 GET；merchant/getCurrentOrg 在本补充内仍同时出现 GET/POST，需核对当前请求。

员工/部门/角色详情 ID 来自已有列表；额度、组织切换、短信和资料写入只用于用户已指定的操作，不作为认证失败的补救。手机号码、员工姓名、验证码等不写入 description/日志。

## 来源详细资料

### 9.8 组织 / 员工 / 部门 / 角色 / 点数（全部 POST，除 GET 标注）

| 请求类型 | 接口 | 功能 | 关键入参 |
| --- | --- | --- | --- |
| POST | `/user-service/employee/list` | 员工列表 | 分页 |
| GET | `/user-service/employee/get/{id}` | 员工详情 | Path `id` |
| GET | `/user-service/employee/del/{id}` | **删除员工** | Path `id`（写，未执行） |
| POST | `/user-service/employee/addOrUpdate` | 新增/改员工 | 员工字段 |
| POST | `/user-service/employee/updateStatus` | 启用/停用 | `id`、`status` |
| POST | `/user-service/employee/makeOver` | 交接 | `fromId`、`toId` |
| POST | `/user-service/employee/generateLink` | 生成邀请链接 | 角色信息 |
| GET | `/user-service/employee/getEmployeeByDeptId/{id}` | 按部门取员工 | Path `id` |
| POST | `/user-service/employee/employeeDeptList` | 员工部门列表 | — |
| POST | `/user-service/dept/list` / `listTwo` | 部门列表 | 分页 |
| GET | `/user-service/dept/get/{id}` | 部门详情 | Path `id` |
| GET | `/user-service/dept/del/{id}` | **删除部门** | Path `id`（写，未执行） |
| POST | `/user-service/dept/addOrUpdate` | 新增/改部门 | 部门字段 |
| POST | `/user-service/dept/updateStatus` | 启用/停用 | `id`、`status` |
| POST | `/user-service/dept/transfer` | 部门迁移 | `fromId`、`toId` |
| POST | `/user-service/role/list` | 角色列表 | 分页 |
| GET | `/user-service/role/get/{id}` | 角色详情 | Path `id` |
| GET | `/user-service/role/del/{id}` | **删除角色** | Path `id`（写，未执行） |
| POST | `/user-service/role/addOrUpdate` | 新增/改角色 | 权限字段 |
| POST | `/user-service/role/updateStatus` | 启用/停用 | `id`、`status` |
| POST | `/user-service/role/transfer` | 角色迁移 | `fromId`、`toId` |
| GET | `/user-service/menu/menuList` | 权限菜单树 | — |
| POST | `/user-service/employeePoints/getAiPoints` | 可用 AI 点数 | 无 ✅ 实测 `{availablePoints:141850}` |
| POST | `/user-service/employeePoints/recordPage` | 点数流水 | 分页 ✅ 实测 |
| POST | `/user-service/employeePoints/typeList` | 消耗类型清单 | 无 ✅ 实测 |
| POST | `/user-service/employeePoints/page` | 成员点数汇总 | 分页 |
| POST | `/user-service/employeePoints/updateTotalPoints` | **调整额度** | `employeeId`、`points`（写，未执行） |
| POST | `/user-service/employeePoints/platUsedPoints` | 平台已用点数 | 时间范围 |
| GET | `/user-service/employeePoints/getAiPointsType` | 点数类型枚举 | — |
| GET | `/user-service/employeePoints/updateAiPointsType?type=` | **切换点数类型** | `type`（写，未执行） |
| POST | `/user-service/user/getMyInfo` | 用户信息 + 菜单 + 机构 | 无 ✅ 实测 |
| POST | `/user-service/user/getCurrentUserInfo` | 当前用户 | 无 |
| POST | `/user-service/user/getInfo` | 基础资料 | 无 |
| POST | `/user-service/user/isOldUser` | 是否老用户 | 无 |
| POST | `/user-service/user/createCompany` | **创建企业空间** | 企业信息（写，未执行） |
| POST | `/user-service/user/createPersonSpace` | **创建个人空间** | 个人信息（写，未执行） |
| POST | `/user-service/user/createImageAuthCode` | 生成图片授权码 | 图片信息 |
| POST | `/user-service/user/checkoutImageAuthCode` | 校验图片授权码 | `code` |
| POST | `/user-service/merchant/getCurrentOrg` | 当前机构 | — |
| POST | `/user-service/merchant/updateName` | **改机构名** | `name`（写） |
| POST | `/user-service/merchant/updateLogo` | **改机构 Logo** | `logoUrl`（写） |
| POST | `/user-service/org/focus/save` | 保存关注机构 | `orgIds[]` |
| POST | `/user-service/org/updateMarket` | 更新市场 | `market` |
| POST | `/user-service/sms/send` | 发短信验证码 | `phone`、`Equipment`、`platForm` |
| POST | `/Account/GetSmsCode` | 登录前验证码 | `phone` |
| POST | `/account-admin/login/product` | 产品侧登录 | 账号密码 |
| POST | `/account-admin/revise/switchWorkspace` | 切换空间 | `orgId` |
| GET | `/sso/api/sso/web/renewToken?refreshToken=` | 刷新登录态 | Query `refreshToken` |
| GET | `/sso/api/sso/web/discardToken` | 退出登录 | — |
