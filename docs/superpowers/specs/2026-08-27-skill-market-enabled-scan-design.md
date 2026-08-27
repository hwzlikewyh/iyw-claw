# 技能市场已启用筛选与对话框目录扫描设计

## 背景

技能市场当前将“已安装”展示为库存专用的左右分栏：左侧是文本行列表，右侧是库存详情。其他市场视图使用卡片网格和详情弹窗。对话框“技能”菜单使用 `useAgentSkills` 的模块缓存，打开菜单时可能继续显示旧的目录结果。

本次改动需要统一已安装视图的主要展示方式，增加紧邻“已安装”的“已启用”视图，并保证对话框打开技能菜单时能看到当前 Agent 和工作目录的最新技能目录。

## 目标与非目标

### 目标

- 在“已安装”旁新增“已启用”页签。
- 已安装和已启用使用与其他市场视图一致的卡片网格、选中态和详情打开方式。
- 保留库存详情中的 Agent 启用矩阵、路径、接管、修复等管理能力。
- 技能菜单打开时，按当前 Agent、当前工作目录扫描全局和项目技能，并更新显示结果。
- 已安装和已启用均过滤项目自带/Agent 内置技能，不显示“托管内容已变更”等陈旧托管提示。
- 不改变后端接口 contract，不改变内置技能与市场覆盖的来源优先级。

### 非目标

- 不新增市场后端查询视图或新的持久化字段。
- 不把纯只读内置观察项重新暴露到“已安装”或对话框快捷菜单。
- 不改变技能启用/停用、接管、卸载和市场安装的业务规则。

## 方案

复用 `skill_inventory_list` 返回的 `SkillInventorySnapshot` 作为已安装和已启用视图的唯一状态来源。库存项过滤全部观察项均为只读或 Agent 内置的项目自带项；含有可管理市场/用户来源的混合项继续保留：

- “已安装”显示所有非纯内置库存项。
- “已启用”显示 `agentStates` 中至少一个 `actualEnabled === true` 的库存项。
- 卡片不渲染 `stale_market_record` 的“托管内容已变更”状态徽标；状态仍保留在库存数据中供管理操作使用。

在前端增加库存卡片列表适配层，将 `LogicalSkillInventoryItem` 渲染为与 `SkillMarketList` 相同的网格密度、卡片容器和交互节奏；卡片选中后打开库存详情弹窗，弹窗内部继续复用当前库存详情组件和操作回调。库存异常状态、Agent 数量和启用状态以现有库存字段为准，不从市场目录推断。

页签状态仍由 `SkillMarketQueryState.view` 管理，并写入 URL。新增 `enabled` 视图值，放在 `installed` 后面。库存视图切换、搜索和刷新沿用现有库存 hook；切换到任一库存视图时读取当前活动工作目录对应的快照。

## 对话框技能扫描

扩展 `useAgentSkills` 的刷新能力，保留按 `agentType|workspacePath` 的缓存和并发去重。消息输入组件在“技能”下拉子菜单从关闭变为打开时触发当前 key 的强制刷新：

1. 使该 key 的旧缓存失效并提高请求代次。
2. 调用现有 `acp_list_agent_skills`，传入当前 Agent 和工作目录；后端扫描全局与项目目录。
3. 请求成功后替换该 key 的缓存并刷新菜单内容。
4. 请求失败时保留上一次成功结果；没有旧结果时显示已有的空状态，不阻塞其他输入操作。

扫描触发只绑定“技能”子菜单的打开事件，不对每个菜单项点击重复发请求。现有 `$`/`/` 自动补全继续使用缓存列表，不改变其触发策略。

## 组件与数据流

```text
SkillMarketView
  -> SkillMarketToolbar (view=installed|enabled)
  -> useSkillInventory(enabled)
  -> InstalledInventoryView
       -> InventoryCardGrid
       -> InventoryDetailDialog
            -> AgentMatrix / SkillLocations / existing mutations

MessageInput
  -> Skills DropdownMenuSub onOpenChange(true)
  -> useAgentSkills.refresh()
  -> acp_list_agent_skills(agentType, workspacePath)
  -> cached skills -> menu items
```

库存详情弹窗关闭后保留原页签、搜索词和选中状态；技能启用或接管成功后刷新库存快照，使“已启用”列表立即收敛。异步请求使用现有 request-id/代次保护，旧请求不得覆盖新工作目录或新扫描结果。

## 错误与边界

- 未加载到库存快照时显示现有加载状态；加载失败时保留现有重试入口。
- “已启用”没有匹配项时显示与市场列表一致的空状态。
- 技能扫描失败不得清空可用的旧菜单列表，也不得把失败误报为“没有技能”。
- Agent 不支持 iyw-claw 技能管理时继续隐藏技能快捷菜单，沿用现有 `supported` 判断。
- 纯内置只读观察项继续从库存页和对话框快捷菜单过滤。

## 验证

- 静态检查新增视图类型在查询解析、URL 持久化、页签渲染和分支渲染中完整覆盖。
- 静态检查库存卡片的筛选条件只使用 `actualEnabled`，且详情操作回调仍连接现有 hook。
- 静态检查菜单打开刷新只传当前 Agent 和工作目录，并保留缓存失败回退与并发代次保护。
- 执行项目允许的定向 TypeScript/ESLint 检查；按仓库 `AGENTS.md` 约定不默认运行单元、集成、端到端或 Rust 测试。
