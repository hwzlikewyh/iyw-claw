# 已发布插件的资源与关闭修复

对已按市场文件摘要校验的发布包执行：

```powershell
node plugins/app-compat/repair.mjs <原发布包目录> <全新的输出目录>
```

支持 Cowart 0.1.28 → 0.1.29、Infinite Canvas 0.1.22 → 0.1.23、
Excalidraw 0.3.6 → 0.3.7，以及 Documents / PDF / Presentations /
Spreadsheets 0.1.0 → 0.1.1。脚本精确校验修补锚点，源版本或源码不匹配时失败；
原包保持只读，不自动上传或安装。

Excalidraw 将完整文档资源迁移为 ui://excalidraw/document/{id}，同步服务端、
UI 和声明；关闭等待已有 flush。Cowart 关闭前等待最后一帧变更进入保存队列及
画布保存，失败继续交给宿主处理。四个 Office 预览释放当前 view。Infinite Canvas
在连接前注册关闭回调，初始化失败也能释放已创建的资源。

复用原权限、依赖、许可证和最低客户端版本；缺少最低版本的 Cowart 使用已验证
发行版 0.1.363。新包保留原市场 ID，通过既有逐文件上传接口发布不可变新版本。
发布前须验证 MCP 资源、浏览器保存/失败行为与文件摘要；发布后下载 ZIP 逐文件复核。

源码入口同时修改 Office adaptPanel 和 Infinite Canvas boot。外部预构建插件采用
受版本约束的作者补丁，后续升级上游版本需重新核验锚点与保存语义。
