# 插件资源与关闭兼容修复

用户已于 2026-11-24 授权修复、提交两端主分支，并发布受影响插件的新版本。

## 验收与边界

- Excalidraw 完整文档可通过现有同 connector、同命名空间的资源模板读取。
- 缺少关闭方法的插件可退出；明确保存失败或超时保留页面，用户可再次确认强制关闭。
- 插件关闭回调在初始化早期注册，等待需要保存的数据，不以吞掉错误伪造保存成功。
- 发布 7 个受影响插件的新版本，保留市场 ID、分发范围、依赖及已有权限；检查新制品与候选一致。
- GitHub main 与内网 GitLab master 均纳入经过验证的本次提交，不带入原工作区其他改动。

## 实现选择

Excalidraw 的 read_scene 只返回摘要，不能替代完整文档读取。将其私有文档 URI
迁移为 ui://excalidraw/document/{id}，同步服务、页面和 native-plugin.json；
保持宿主的资源权限边界。其关闭回调等待已有场景控制器 flush。

宿主仅把 JSON-RPC -32601 识别为未实现可选关闭方法。其余协议错误及 15 秒超时
仍视为关闭失败，明确提示用户可强制关闭且可能丢失未保存内容；取消保留页面。
点击关闭前设置同步锁，避免 React 状态更新前重复弹窗。

四个 Office 预览插件关闭时释放当前 view；Cowart 等待画布持久化；无限画布在
连接前注册回调，初始化异常也可清理。包修复通过可复用作者脚本生成新目录，
不会原地覆盖发布包；后续制包入口同步修正。

## 计划与验证

1. 已完成：宿主和插件修复 → 静态调用链审查、语法与类型检查、真实 MCP 及浏览器交互验证。
2. 已完成：生成 7 个新版本 → 核对每项文件差异、资源模板、保存成功/失败与初始化失败关闭。
3. 已完成：双主分支提交合并 → 检查专属 diff、两端最新历史及远程提交包含关系。
4. 已完成：市场发布 → 原 ID 上传、制品 ready、公开列表、升级检查及 ZIP 摘要核验。

遵循仓库约定，不新增测试文件或执行测试套件；验证脚本使用临时目录，结束清理。
官方 MCP Apps App.onteardown 文档要求先 await saveState 再释放连接并返回结果：
https://apps.extensions.modelcontextprotocol.io/api/classes/app.App.html#onteardown

验证记录：TypeScript、定向 ESLint、Prettier 和 diff 检查通过；7 个候选的真实 MCP/浏览器保存关闭检查通过，9 个既有界面插件关闭回归通过。Excalidraw 和 Cowart 的实际编辑内容已从隔离存储回读；模拟保存失败均拒绝关闭。宿主正常/未实现/错误/超时/断开协议和取消/强制关闭/双击路径已定向验证。验证证据位于 D:/projects/iyw/reports/plugin-resource-fix-20261124。

生产前端完整编译、TypeScript 检查和静态导出成功。pnpm 启动时因共享依赖目录一致性检查拒绝非交互重装，已按 package.json 的同一 prebuild 与 next build 命令直接执行，未重装或更改依赖。Tauri onCloseRequested 官方 SDK 源码确认会等待异步 handler 后再决定销毁窗口。

交付：GitHub main 代码提交 7de9da172，GitLab master 代码提交 e26e020b6，两端远程回读确认。7 个插件新版本均 ready，普通用户市场可见且旧版本检查返回 update_available；下载 ZIP 共 769 个文件与候选逐项一致。版本为 Excalidraw 0.3.7、Cowart 0.1.29、Infinite Canvas 0.1.23，以及四个 Office 预览插件 0.1.1。未发布客户端安装包；宿主补丁已进入两端主分支。
