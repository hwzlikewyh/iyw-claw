# CLI 连接器通用适配器

将 WorkBuddy 快照中的 49 个 CLI 连接器转换为原助理现有的连接器制品和 CLI 登录声明。
44 个纯 CLI 通过参数数组调用工具，5 个已有 MCP 的连接器保留官方 MCP 入口。
当前只声明 Windows x64；其他平台未经验证，不出现在安装选项中。

## 运行方式

宿主提供 Node.js 22+，Python 连接器还需要受管 uv。
宿主将 HOME、USERPROFILE、APPDATA 和 XDG 路径绑定到账号目录，
并设置 `IYW_CONNECTOR_AUTH_DIR`。各入口共享账号目录和版本独立的 `.runtime`：

```powershell
node adapter.mjs prepare
node adapter.mjs login
node adapter.mjs status
node adapter.mjs logout
node adapter.mjs serve
```

- `prepare` 安装固定版本依赖、复制原生程序、核对版本和全部命令入口。
- `login` 依次执行授权步骤，支持 `skipIf`；准备时间计入宿主五分钟登录窗口。
  下载耗时较长时保留已下载内容，用户可重新登录继续。
- `status` 按供应商 JSON 字段或明确的状态文本判定，未登录退出 1。
- `logout` 使用官方退出命令。企业微信没有退出子命令，仅清理专用授权目录，
  校验真实路径属于当前账号后才清理；不会删除系统默认配置。
- `serve` 提供 MCP 或转接官方 MCP。工具参数不经过 shell，取消与超时会结束子进程树。

原生程序在账号运行目录执行，避免自解压或自更新改写共享制品缓存。
企业微信使用官方 `WECOM_CLI_CONFIG_DIR`；云之家使用账号派生的独立 profile。
飞书使用 `LARKSUITE_CLI_CONFIG_DIR`，官方 Windows 凭据仍由其 DPAPI/注册表实现管理。
目录隔离不代表所有供应商的系统钥匙串也已隔离。

## 构建

先准备官方源码快照、npm 元数据与原生制品；`recipes/` 保存审核后的版本、来源摘要和命令。
资源 `source` 使用相对路径。`assets/` 对应准备后的官方制品目录，
`workbuddy/` 对应同一 WorkBuddy 快照的 `connectors/` 目录；通过构建环境变量绑定，
配方不包含构建机绝对路径。任何文件摘要变化都要求重新审核后导入。

```powershell
npm ci
python tools/prepare_sources.py --output <官方制品目录>
python tools/prepare_native.py --output <官方制品目录> --metadata <官方npm元数据目录>
python tools/import_workbuddy.py --source <WorkBuddy快照> --metadata <官方npm元数据目录> --assets <官方制品目录> --output recipes
$env:IYW_CONNECTOR_SOURCE_ROOT = "<WorkBuddy快照>/connectors"
$env:IYW_CONNECTOR_ASSET_ROOT = "<官方制品目录>"
npm run build
python tools/package_artifacts.py --output <发布制品目录>
```

导入结果必须全部为 `converted: true`；构建会核对随包资源的 SHA-256。
每个 `dist/<连接器>/` 包含适配器、运行清单、安装选项和必需的供应商文件。
打包只收录声明文件，验证 ZIP CRC，输出文件大小和 SHA-256 清单。
安装包和解包总字节数不设固定上限；保留宿主文件数量限制与路径、摘要校验。
商龙直接包含完整原生程序，其 `--help` 由适配器返回供应商随包的命令参考。

将 ZIP 交给既有连接器对象存储直传流程，取得制品 ID 后填写
`install-option.json` 的 `artifact_id`，再更新对应连接器版本。
构建和打包不会上传、发布或覆盖已安装账号配置。
宿主约定见 [连接器市场](../../docs/connector-market.md)。

## 验证边界

2026-11-26：49 个连接器均完成 Windows 运行环境准备，空账号状态检查均返回未登录。
44 个纯 CLI 均通过 MCP 初始化、工具发现和帮助调用；3 个官方 MCP 通过工具发现。
AI-HIVE、微生活在未授权时由官方服务拒绝初始化或工具发现，须授权后继续验证。
没有执行真实业务写操作，没有完成各平台账号授权，也没有发布市场更新。

按仓库约定未新增或运行测试套件；执行了适配器构建、语法与 lint、实际入口检查。
完整客户端 Rust 和 TypeScript 检查仍受工作区其他改动的错误阻挡，不能据此声明整包构建通过。
