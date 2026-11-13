# 桌面／服务器宿主：市场与插件管理接口

本页供现有界面和受授权客户端对接，不能把这些内部路由当作Agent直接工具。Agent优先用 [manage_iyw_connectors](connector-management.md)、当前插件安装／启用能力及UI。服务端路径为POST /api/<命令名>；桌面端用同名Tauri invoke，参数结构相同，由现有transport负责选择。

使用当前宿主认证；不把云用户token当宿主口令，也不在网页直连服务端私有文件路径。云市场是另一服务，返回code/data；宿主成功响应为具体JSON对象、数组或null，失败沿AppCommandError，不能套用Fusion响应解析。

`skill_market_publish` / `skill_market_add_version` 接收本地文件参数后，计算摘要、初始化清单并逐文件直传 TOS，最后只向 Fusion 提交 uploadId；不再组装 multipart。新建 request 支持可选 audience。Agent 已有本地目录时可直接按 [市场上传](market-upload.md) 的脚本与 JSON API 发布，不依赖文件夹选择框。连接器目录发布的宿主命令是 `connector_market_user_request`，请求 `{request:{action,body}}`，action 与 [连接器用户接口](connector-publishing.md) 对应，最终同样走用户登录态和 TOS 直传。

## 技能／插件市场：12个接口

下表花括号是请求JSON，尖括号表示真实值占位。完整枚举以项目当前类型为准。

| 命令名 | 请求 | 结果／用途 |
| --- | --- | --- |
| skill_market_list | {params:{page?,pageSize?,view,visibility?,publisherType?,category?,q?}} | 市场分页；params按当前SkillMarketListParams |
| skill_market_categories | 无参数 | 分类数组 |
| skill_market_check_updates | {items:[{id,installedVersion}]} | 更新候选 |
| skill_market_detail | {id,version?} | skill、files、installTargets |
| skill_market_list_versions | {id} | 云版本数组 |
| skill_market_publish | {request:<PublishRequest>} | 初次发布，返回SkillMarketDetail |
| skill_market_add_version | {request:<AddVersionRequest>} | 发布新版，返回详情 |
| skill_market_update_metadata | {request:<MetadataRequest>} | 修改元数据与audience |
| skill_market_delete | {id} | 删除云市场项目；成功null |
| skill_market_install | {id,version,agentTypes:[<实际枚举>]} | 通过现有安装器安装；成功null |
| skill_market_uninstall | {id} | 卸载本地安装；成功null |
| skill_market_rebuild_artifact | {id,version} | 现有制品修复入口；按权限及服务支持处理 |

PublishRequest = {slug,displayName,summary,category,iconUrl?,tags,visibility,version,changelog,packageType?,dependencies?,files}。files每项为{path,contentBase64,size}，dependencies每项{slug,version}。该宿主创建结构没有audience；创建后用元数据请求显式设置受众。

该宿主发布器走普通用户 API，支持 v1/v2 插件及技能/专家包。不要将管理凭据加到宿主 request，或把 packageType 改成错误类型。

AddVersionRequest = {id,version,changelog,packageType?,dependencies?,files}。

MetadataRequest = {id,displayName,summary,category,iconUrl?,tags,visibility,audience}。它要求完整元数据，不能把云 /skills/update 的局部更新请求直接套用。

云逐文件直传适合大插件；宿主 Base64 输入最多512文件，仅用于本地解码，随后原始字节直接 PUT TOS。大量文件优先用目录脚本，避免整个包进入工具参数；同一版本保持一个上传会话。Skill/Expert 的服务端总量限制和 Plugin 清单校验仍然有效。

## 插件准备：6个接口

以下全部为 {input:{...}} 包装：

| 命令名 | input字段 | 结果 |
| --- | --- | --- |
| plugin_preparation_get | marketSkillId、conversationId可选 | PreparationView，含state、issues、dependencies、permissions、connectors、contextToken |
| plugin_preparation_configure | marketSkillId、conversationId、contextToken、connectorKey、values:字符串map、clearKeys可选 | 新PreparationView |
| plugin_preparation_account | 同上对象定位＋operation | AccountStatus；操作按当前AuthOperation |
| plugin_preparation_approve | marketSkillId、conversationId、contextToken | 批准并返回准备状态 |
| plugin_preparation_disable | 同approve | 禁用后状态 |
| plugin_preparation_check | 同approve | ConnectionView：state、toolCount |

账号operation为status／login／logout，connectorKey必需。配置values中的秘密只从受保护表单进入宿主安全存储；Agent不把它们写进工具参数或会话。返回参数含configured，secret不回显。

read → 展示当前上下文 → configure／approve／account → 再read。contextToken必须来自最新get；换项目、账号、版本或配置后不能复用。check的toolCount仅为连接工具数，不是登录证据。

## 旧安装恢复：2个接口

| 命令名 | input | 注意 |
| --- | --- | --- |
| plugin_recovery_inspect | {marketSkillId} | 返回state、scopeChanged、resetPermissions、credentialsRetained、listed、reviewToken、issues、dependencies |
| plugin_recovery_apply | {marketSkillId,reviewToken} | 对应本次inspect；不猜token |

先显示恢复影响，按已有授权执行。scope变化、摘要缺失、依赖不可用等按issues处理，不能以重装最新版本代替“恢复原版”。

## 本地版本切换：3个接口

| 命令名 | input | 结果 |
| --- | --- | --- |
| plugin_versions_list | {marketSkillId,page?} | currentVersion、versions、nextPage |
| plugin_version_inspect | {marketSkillId,version} | reviewToken、scopeChanged、permissionsChanged、preparationChanged、dependencies、related、issues |
| plugin_version_apply | {marketSkillId,version,reviewToken} | 指定本地版本切换结果 |

只使用返回available=true的实际版本；版本切换和当前市场最新版本不是一个概念。nextPage非空继续读该页；不要把云versions/list当成本地保留版本证明。

## 插件卸载与清理：5个接口

| 命令名 | input | 结果／用途 |
| --- | --- | --- |
| plugin_remove_inspect | {marketSkillId} | RemovalReview：affected、retained、warnings、clearAvailable、parameterRecords、reviewToken等 |
| plugin_remove_apply | {marketSkillId,reviewToken,clearAccountData:boolean} | 按选择卸载，保留共享依赖 |
| plugin_remove_status | {marketSkillId} | 最近RemovalReview或null |
| plugin_remove_cleanup | {marketSkillId} | 重试未完成清理 |
| plugin_remove_recover | {marketSkillId} | 按宿主既有恢复流程处理卸载状态 |

clearAccountData必须体现用户对账号数据清理的真实选择；默认不要从“卸载”推导“清除所有凭据”。先inspect，再apply；迟到响应不能覆盖更新的scope／review。被其他插件共用的组件按retained保留，不自行递归删目录。

## 接口调用示例

前端复用项目已有transport的invoke：

```typescript
const view = await invoke("plugin_preparation_get", {
  input: { marketSkillId: selectedId, conversationId },
})
```

其中invoke来自项目既有transport，selectedId／conversationId来自当前真实界面状态。对服务端发相同JSON到POST /api/plugin_preparation_get。

外部客户端不要绕过现有UI的用户批准、权限审查、contextToken和reviewToken流程。制作可审核请求后沿宿主操作，结果必须回读确认。

来源：web/router.rs、web/handlers/{skill_market,plugin_preparation,plugin_history,plugin_recovery,plugin_remove}.rs；commands/skill_market/{types,plugin_history/types,plugin_remove/types}.rs、commands/{plugin_preparation/types,plugin_recovery/types}.rs。
