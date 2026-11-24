# 内嵌 Office 预览资源

主程序直接链接 OfficeCLI-rust 的 `hcd-formats` 和 `hcd-core`，资源与该 Rust 提交固定一致。解析不启动 CLI，不依赖用户安装 OfficeCLI。

前端使用上游通用懒加载视图和 XLSX/Univer 适配器，保留哈希、节点及资源校验。表格只读，分片按可视区域读取；不调用 Univer 商业导入导出。预览资源随应用编译，不在打开文档时访问网络。

重建（仅开发者）：

```powershell
npm ci --prefix scripts/office-preview
node scripts/office-preview/build.mjs
```

也可以向构建脚本传入固定提交的本地 OfficeCLI-rust 源码根目录。脚本将换行归一后逐文件验证固定摘要，输出到 `src-tauri/resources/office-preview/`；依赖使用本目录锁文件，不改变应用前端依赖。

输出为应用构建资源，需与源码一起审查和保留。`upstream.json` 记录来源、版本、摘要和修改说明；`LICENSE-OfficeCLI.txt` 保留 Apache-2.0，`grid.js.LEGAL.txt` 与 `licenses.json` 保留实际打包依赖的声明和许可。授权的预览会话可从同一预览前缀的 `licenses?cap=...` 读取许可。

Word/PPT 通过上游 HCD 语义/固定几何视图显示，不承诺与 Microsoft Office 像素一致。人工编辑继续经 Agent 的 OfficeCLI-rust 工具修改原文件，文件稳定后应用重新解析并主动通知视图更新。

当前固定Foldly v0.1.32，document-viewer.js在同一沙箱的Shadow DOM中渲染Word/PPT。构建替换点有精确计数校验；Excel适配器补充活动工作表与销毁状态约束，保留只读内容栏。入口仍使用grid.ts.in模板，升级时对替换后的源码单独执行严格类型检查。

本地排版扩展：`word-pagination.mjs` 组装 Word 分页；`grid-layout.ts.in` 按原样式对窄单元格缩字，与入口一样在构建缓存中生成 TypeScript，避免主项目解析仅在构建时存在的上游模块；内置引擎 assets 下的 `slide-layout.js` 和 `office-fonts.js` 同时供实时视图与独立导出使用。源文件裁剪、缩字规则由 `preview_styles.rs` 提取为派生 CSS，不修改已校验的 HCD 内容。
