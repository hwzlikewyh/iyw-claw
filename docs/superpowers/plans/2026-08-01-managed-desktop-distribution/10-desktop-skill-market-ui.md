# Task 10：桌面 Skill 市场 UI 与性能

## 目标

在不改变后端契约的前提下重做桌面 Skill 市场信息架构和交互，使官方、组织、私有、强制、兼容和安装状态可快速识别；优化大列表、详情和下载进度性能。

## 依赖

- Task 03 冻结 Skill list/detail/install plan DTO、audience/distribution/artifact 状态和错误 code。
- 可以先使用 typed fixture 开发，禁止自行改后端字段。

## scope_write

- `src/app/settings/skills/`
- `src/components/skills/`
- Skill 市场专用 hooks/state/view-model
- `src/lib/skill-market.ts` 的展示映射；底层 ticket/download contract 变更需交 Task 03
- 对应 i18n 消息

## 禁止修改

- Rust 安装器、后端、全局设置导航、根样式/依赖/lockfile。
- 在 UI 中自行重新判断权限或兼容；只展示服务端/安装器有效状态。

## 受众与工作流

主要用户需要反复完成：发现 -> 比较 -> 查看依赖/兼容 -> 安装/更新 -> 观察进度 -> 打开或卸载。界面应是安静、紧凑、可扫描的工作台，不做营销 landing page。

## 布局

- 桌面：顶部紧凑工具栏 + 左侧可虚拟化结果列表 + 右侧详情，稳定列宽；详情不是嵌套卡片堆叠。
- 窄屏：列表和详情分屏导航，保留筛选状态；不使用动态路由，继续符合 Next 静态导出，使用查询参数。
- 固定工具栏高度、列表行高度/最小高度和操作区尺寸，加载/徽标/长名称不引发布局跳动。
- 圆角不超过 8px，页面 section 不包装成悬浮卡，卡片仅用于真正重复 Skill item。

## 顶部工具栏

- 搜索输入，300ms debounce，可清除。
- 视图 segmented control：市场、组织、我的、已安装、需要更新。
- 筛选菜单：官方/用户、可选/强制、兼容/不兼容、分类。
- 排序：推荐、最近更新、名称、已安装状态。
- 刷新使用图标按钮和 tooltip；显示 catalog revision/离线状态但不堆说明文字。
- 筛选写入 query state，返回页面不丢失。

## 列表项

每项显示：

- 名称、简短摘要、图标 fallback。
- 官方/组织/私有 audience 标识。
- 强制/可选、已安装、可更新、准备中、已阻断。
- 当前版本与已安装版本。
- 兼容状态和一个主要动作。

徽标颜色不能只靠色相表达，包含文本/图标；不要同时显示超过必要数量，次要信息进入详情。

## 详情

- header：名称、publisher、audience、版本选择、主操作。
- overview：摘要、标签、更新时间、来源组织。
- compatibility：PC 版本、OS/arch、依赖、强制原因和 deadline。
- versions：发布说明、artifact ready 状态、大小。
- files：按需加载树，大树虚拟化/折叠；不随列表请求返回。
- ownership：系统/市场/用户目录和是否受管，不暴露绝对路径。

主按钮状态严格来自 install plan：安装、更新、已安装、等待制品、版本不兼容、被策略阻断。未知状态安全禁用并允许刷新诊断。

## 安装体验

- 点击前先 resolve，显示最终版本、依赖数、下载量和是否强制。
- 依赖安装使用一个总任务，并可展开每个 artifact 的下载/校验/安装/激活状态。
- 支持取消可取消阶段；激活阶段明确不可取消。
- URL 过期刷新在后台完成，不把用户带回详情。
- 失败显示 error code 对应动作：重试、释放空间、更新 PC、联系管理员、查看诊断。
- 成功后局部更新当前 item/detail/inventory，不全页 reload。

## 上传与管理

- 用户上传 audience 只能选择组织或本人私有；官方 global 由后台发布。
- 显示原始文件上传与 artifact 构建两个阶段，complete 不等于可安装。
- artifact failed 显示重建/联系管理员，不提供错误的“下载 ZIP”。
- 版本、依赖和 audience 修改受后端 canManage 控制。

## 性能

- 列表请求可取消，旧响应不能覆盖新筛选结果。
- 结果分页或 cursor + virtualization，不渲染全市场 DOM。
- detail/version/files 分层 cache，以 catalog revision 失效。
- 图标 lazy load、限制尺寸；失败用本地 fallback。
- 进度事件节流到约 5-10Hz；字节累计不触发整个页面 rerender。
- memoize 大文件树的 flatten 结果，避免每次进度更新重算。
- 记录 list first-content、search response、detail open 和 action-ready 的 P50/P95。

## 无障碍与响应式

- 所有 icon button 有 accessible name/tooltip。
- 键盘可搜索、切视图、移动列表选择、打开详情和执行主动作。
- focus 不因数据刷新丢失。
- 1280x800、1440x900、1920x1080 和窄屏无重叠/横向溢出；长中文/英文单词可换行或截断并 tooltip。
- 不用 viewport width 缩放字体，letter spacing 为 0。

## 验证

- typed fixture 覆盖所有 audience/distribution/artifact/compatibility/install 状态。
- 远端 Playwright 截图和交互检查；本机不启动桌面/前端 build。
- 500/5000 条列表性能基准，滚动无明显卡顿，筛选旧请求不会覆盖新请求。
- 安装断网、制品准备中、强制更新、依赖失败、磁盘满、离线 cache UI 完整。

## 完成定义

- 用户无需猜测即可区分官方、同组织、私有和强制 Skill。
- 大列表和进度更新不拖慢整个设置页。
- 所有状态有稳定布局、正确动作和可恢复错误。
