# 内置 OfficeCLI-rust 命令解释器

来源：RainLib/Foldly（原OfficeCLI-rust）v0.1.32，提交1846fb35fcca4b5dcc49525089d1030578c3b8e0，Apache-2.0。

上游只有二进制crate，没有公开CLI库。本目录保留其命令层、schema与技能作为可链接库，文档处理crate继续从固定git提交依赖。未复制测试目录与上游构建产物。

本地修改：main入口改为显式参数的run；公开bundled_skill；所有自身重启路径添加宿主的--embedded-office前缀；禁用独立二进制安装命令；build.rs资源根改为当前crate。v0.1.32新增的导出、集成发布和MCP注册入口也保留宿主前缀；帮助用HCD补丁schema复制到schemas，内置管理页复制到assets/catalog-admin.html。应用启动前分派办公命令，不启动GUI或服务端，不连接业务后台。普通预览则直接调用HCD库，不执行命令入口。

v0.1.32新增office-engine与canvas-core库，命令解释器仍由本目录适配；全部上游crate固定到同一提交，不混用0.1.31处理器。上游删除的commands/html_convert.rs已随本次同步移除，HTML转换使用其新的office-engine库。

按宿主仓库规则，不随命令层复制上游新增的独立tests.rs及其cfg(test)模块声明；现有源码中的条件编译测试块保留。

上游既有源码/函数长度按第三方代码维护，本地入口放在宿主office_embedded.rs。升级时需核对所有current_exe调用、命令协议、技能、schema与许可，禁止自动更新。

排版补充由 `word_preview.rs`、`preview_styles.rs` 和 assets 下的独立脚本实现：共享 Word 分页、PPT 等比适配与图片裁剪、Excel 缩字。CLI 与应用继续共用固定版本 HCD 库，派生显示样式不回写 Office 文件。浏览器截图与 PDF 先写临时输出，确认完整后发布目标文件。
