# Intelligent UI

重启使用新代码构建的应用后，在 **设置 → 外观 → 智能交互界面** 开启。
默认关闭；开关保存在当前本机或服务器数据库，向连接到它的窗口广播。
关闭后代码围栏按普通代码显示，已保存的参数保留，后续消息不注入 UI 状态。

开启后可发送“做一个可以调整本金、年利率和期限的房贷计算面板”。
内置 `ui-render` Skill 在每轮私有上下文中说明输出格式和组件能力。
具体结果仍取决于当前代理与模型是否按照该协议生成回复。

## 两种输出

`iyw-ui` 是结构化 JSON，支持布局、表单、公式、图表、表格、测验、决策树、
甘特图、差异、函数图与 SVG 场景。表达式由受限解释器执行，支持预算和深度限制。

````markdown
```iyw-ui
{"version":2,"component":"panel","id":"budget-1","state":{"quantity":3,"price":100},"computed":{"total":"quantity * price"},"children":[{"component":"slider","bind":"quantity","label":"数量","min":1,"max":20},{"component":"input","bind":"price","label":"单价","type":"number"},{"component":"stat","label":"合计","value":"{total}","unit":"元"},{"component":"button","ask":"按数量 {quantity} 和单价 {price} 给出建议"}]}
```
````

`iyw-widget` 是含内联 HTML/CSS/JavaScript 的图解。只有完整闭合的围栏会启动。
模型代码运行于内层 `sandbox="allow-scripts"` iframe；可信外层转发经过来源和
随机 token 校验的消息，限制内层页面跳转。CSP 限制外部资源、连接、嵌套框架、
worker、表单与插件。iframe 无同源权限，不接入应用工具和文件接口。

widget 可使用 `window.iyw.getState/onState/setState` 保存用户参数，以及
`suggest/ask` 建议追问和 `onTheme` 接收主题。追问在宿主中编辑并点击发送，
不会自动调用代理或工具。

## 交互和恢复

- 参数、表格搜索/排序/页码、卡片筛选、图例、测验答案、决策路径与函数图设置
  进入同一 block 的状态存储。放大结构化界面与原界面共享参数。
- 表格 CSV 导出包含当前筛选和排序后的全部行；block 导出 JSON 包含源码与当前状态。
- 状态按 `conversation_id + block_id + source_hash` 隔离；`turn_id` 记录来源。
  这样历史解析器重新编号、会话分叉后仍能匹配相同源码。
- 保存使用 revision 比较。其他窗口的修改不会覆盖本地未保存输入；冲突时可重试，
  或选择“重新加载选择”接受数据库版本。发送消息前刷新待保存状态。
- 分叉事务将原会话状态复制到原历史的兄弟会话；新分支只注入实际存在于其历史中的块。
  被裁掉的后续回复不会进入新分支上下文。彻底删除会话时通过外键级联移除状态，
  软删除保留状态。现有完整数据库备份包含开关和状态表。
- 完全重复、无法唯一对应到助手回复的源码不能写入状态，会显示保存错误。
  模型应在同一对话使用唯一 id；源码不匹配或用户/工具输出不能成为状态保存来源。

每块状态最多 4 KiB、4 层；源码最多 200 KB；每会话最多保存 256 块。
下一轮只携带本分支最近至多 8 个有效块的状态，状态片段总预算 8 KiB。
`__view_` 前缀由宿主保留。状态被当作用户数据，经过 XML 转义，不作为授权或指令。

## 接入位置

| 位置                                          | 职责                                         |
| --------------------------------------------- | -------------------------------------------- |
| `src/lib/intelligent-ui/engine/`              | 解析、编译、计算、点公式、过滤与状态差异     |
| `src/components/intelligent-ui/`              | 原生组件、作用域、状态 hook、图解隔离        |
| `src/lib/intelligent-ui/`                     | Transport 调用、设置、缓存、事件、保存和导出 |
| `src-tauri/src/commands/intelligent_ui/`      | 校验、持久化、状态上下文                     |
| `src-tauri/src/acp/manager.rs`                | 每轮能力注入、分叉状态复制                   |
| `src-tauri/experts/skills/ui-render/SKILL.md` | 模型输出约定和完整组件目录                   |

Tauri 命令与 Axum POST 端点共用 `_core` 函数：
`get_intelligent_ui_settings`、`set_intelligent_ui_settings`、
`read_intelligent_ui_states`、`save_intelligent_ui_state`。
远端 Transport 切换时清空旧连接缓存，WebSocket 重连后重新读取已保存状态。

引擎基于 DEEIX-Chat 的 Apache-2.0 源码，来源、修改说明、LICENSE 与 NOTICE
位于 `src/lib/intelligent-ui/`；没有新增 npm 依赖。

## 验证方式

按照本仓库 AGENTS.md，本次不新增或运行测试。交付检查为调用链静态审阅、
TypeScript、定向 ESLint、前端静态导出与两种 Rust 运行模式的 `cargo check`。
若默认 Turbopack 遇到构建器内部超时，可使用已安装的 Next CLI：

```powershell
node node_modules/next/dist/bin/next build --webpack
```

真实代理生成、桌面 WebView 与浏览器的交互尚需实际使用时核对；编译检查不代替
这些运行时行为的验证。

### 本次检查结果

- 全量 TypeScript、定向 ESLint、diff 空白检查通过。
- 新 TypeScript 模块的文件 ≤300 行、函数 ≤50 行、位置参数 ≤3；静态检查通过。
- 内嵌 widget runtime/relay JavaScript 的语法解析通过，没有执行脚本。
- 较早的 server-runtime/tauri-runtime 检查均通过；最终工作区新增的并行改动使服务端遇到 `managed_environment.rs` 对仅桌面开放的 `preferences` 的引用，桌面遇到图片模块 `parameters_schema` 的私有重导出。最终全仓库 Rust 检查不能标为通过。
- 默认 Turbopack 在全局 CSS 子进程通信超时；Webpack 静态导出长时间无进度、内存约6.3 GB后停止，构建未验证完成。
- 按仓库要求没有新增或运行测试，真实代理、浏览器/WebView 交互尚未验证。
