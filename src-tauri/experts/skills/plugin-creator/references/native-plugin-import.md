# 原生插件导入

适用于用户要求在原助理中接入现有 Codex/Claude 原生插件，且插件带有已构建的
Node.js stdio MCP 服务。需要 Node.js 22 或更高版本，不需要额外 npm 依赖。

在本技能目录运行：

```bash
node scripts/import-native-plugin.mjs --source <native-plugin-directory> --output <output-parent-directory>
```

输出为 `<output-parent-directory>/<plugin-name>`。选择新的输出位置；已有目录
不会被覆盖。导入器会运行源包中的 MCP 服务，读取 `tools/list`、`resources/list`
与界面资源来生成契约，不调用登录、生成或付费工具。仅处理用户指定并授权使用的源包。

使用插件自己的正式发布包或已构建目录。`npx --package=<name>@<version> <bin>`
会映射到同版本源包中的 `package.json.bin`，导入后的运行时无需 npx 或联网安装。
支持 `node <entrypoint>`；依赖须已打包到构建产物中。导入器从复制后的制品捕获
契约，缺失 node_modules 等运行依赖会失败。不要通过恢复安装脚本来绕过失败。
插件清单中的密码、Cookie、Token、API key 和认证值必须使用 `${ENV_NAME}`
形式引用宿主变量，不得写入插件包；源 `.mcp.json` 不会被复制到导入制品。
源目录内的 `.env*` 与私钥文件也会跳过。

导入完成后，在原助理插件市场现有的上传入口选择整个输出目录，保留原文件夹名称，
再安装并授予插件权限。发布到市场属于外部写入，须遵守用户已有授权；用户仅要求
生成导入包时，到交付目录为止。现有安装器继续负责摘要、大小、契约、版本和权限校验。

技能中的工具名通过 `search_iyw_capabilities` 和 `invoke_iyw_capability` 使用。
应用专属工具只能由关联界面调用。插件声明的环境变量需在运行原助理的进程中设置；
已声明且以 `_DATA_DIR` 结尾的变量默认指向插件私有持久目录。Blender 等可选外部程序需自行提供。服务器模式使用服务器上的环境与文件，不复用浏览器
客户端的本地登录和程序。

HTTP/SSE、任意 shell 命令、hooks、commands、agents、外部安装步骤及没有相应
构建产物的 npm 服务不在此导入器范围，遇到时明确报告具体边界。不要宣称所有生态插件
都已兼容，也不要以只读 MCP 握手代替账号登录、付费生成或原生界面验收。
