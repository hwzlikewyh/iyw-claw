# 插件包制作、导入、准备、授权与运行

适用于IYW插件市场；上传协议见 [上传](market-upload.md)，公开／组织／个人分发见 [市场入口](capability-market.md)，宿主接口见 [对接](market-host-integration.md)。有当前启用的 plugin-creator 时按其作者流程制作；本页说明发布必须满足的IYW契约。

## v1与v2

| 版本 | 用途 | 运行方式 |
| --- | --- | --- |
| schemaVersion=1 | 可移植技能＋MCP连接器 | targets按已有契约为codex、claude_code；宿主注册组件 |
| schemaVersion=2 | HostGateway插件，带运行时、能力或MCP Apps | targets为iyw-claw；能力经宿主目录和权限执行 |

不要手工把v1的schemaVersion改2就称完成迁移。市场详情返回的plugin是服务端规范化结构（components为数组、含bindings和manifestDigest）；它不是可直接保存为原始 .iyw-plugin.json 的作者格式。

普通用户可用 `/skills/uploads/*` 上传 v1/v2 插件，当前登录态即可；文件逐个直传 TOS，完成后校验清单再提交。用户发布保留 user 身份，官方标识仍由管理员维护。

v2 组织分发的目标来自上传用户的组织身份；个人受众属于实际创建者。普通用户可以发布自己的 owner_private 包，不能借该字段转移给任意用户，详见[分发边界](capability-market.md)。

### v1最小包

```text
my-plugin/
  .iyw-plugin.json
  .codex-plugin/plugin.json
  .claude-plugin/plugin.json
  .mcp.json
  skills/my-skill/SKILL.md
```

三份清单的name／version保持一致；name与市场slug按作者校验器保持一致。Codex清单显式声明组件路径；Claude沿其既有发现规则。只保留实际存在的可选组件路径。

```json
{
  "schemaVersion":1,
  "name":"my-plugin",
  "version":"1.0.0",
  "targets":["codex","claude_code"],
  "components":{
    "skills":[{"key":"my-skill","path":"skills/my-skill","requiresConnectors":["docs"]}],
    "connectors":[{"key":"docs","serverKey":"docs"}]
  }
}
```

skills路径是skills/下的直接子目录，且含SKILL.md；serverKey与.mcp.json中的mcpServers名称准确对应；requiresConnectors引用本清单connector key。原生清单不能包含真实凭据。

### 原生插件导入v2

现有原生Codex／Claude插件若包含**已构建Node.js stdio MCP服务**，用内置plugin-creator的导入器；从该skill根运行：

```bash
node scripts/import-native-plugin.mjs --source <原生插件目录> --output <新的输出父目录> --min-client-version <已验证宿主发行SemVer>
```

需要Node.js≥22；导入器不覆盖已有输出。它运行获授权源包的MCP服务捕获tools/list、resources/list和界面资源，不调用业务登录、付费生成等工具。制品必须已构建并带齐运行依赖，不能假定目标机联网npm安装。

当前导入范围不含HTTP/SSE服务、任意shell、hooks／commands／agents和外部安装步骤；不是所有原生生态插件都兼容。导入跳过源.mcp.json、.env和私钥等敏感文件；认证变量保留为 ${ENV_NAME} 引用。

v2输出包禁止携带.codex-plugin/plugin.json、.claude-plugin/plugin.json和根.mcp.json；原生内容由导入器转换成HostGateway契约，不把源目录三份配置原样拷回输出。v1的“三份清单”规则不能套到v2。

正式上传交互v2插件必须声明minClientVersion，完整SemVer；不把当前开发版本猜为最低可用版本。门槛、platforms、准备声明参与不可变清单摘要，改变这些声明要发新版。

## v2声明需核对什么

按已有生成器／作者校验器生成，不手写未经验证的完整v2 schema：

- **runtime**：真实制品路径、入口、受支持运行时；不依赖安装后下载未知代码。
- **connector**：运行时和已捕获MCP工具／资源契约，key稳定。
- **capability**：实际工具绑定、输入schema、读写注解及调用归属。
- **app**：实际UI资源及关联工具；应用专属工具只能由关联界面调用。
- **permissions**：workspace.read／write、network.connectDomains／resourceDomains／frameDomains、host。只声明实际业务所需范围。
- **preparation**：每连接器的字符串参数和可选业务账号status／login／logout；参数默认值不包含秘密。

platforms支持windows／macos／linux的x86_64／aarch64及两种Win7组合；只列验证过的平台。targets是Agent支持，platforms是运行宿主平台，minClientVersion是宿主二进制门槛。

账号声明中的status必须绑定同一连接器的只读能力，发布声明和运行时工具注解都要满足只读要求。账号判断通过声明的JSON Pointer读取布尔值，不用任意成功字符串推断已登录；login／logout各自执行后仍要status确认。

## 作者验证

v1作者流程从plugin-creator根运行：

```bash
python3 scripts/validate_plugin.py <插件目录>
```

v2准备声明使用同skill中的校验器：

```bash
node scripts/validate_plugin_preparation.mjs <插件目录>
```

具体参数以校验器当前usage为准。原生导入器已经生成并捕获契约时，不使用仅适用于v1脚手架的校验器替代v2校验。校验目录结构和契约通过不等于真实业务登录／生成已通过。

上传整个输出插件目录，也可用 [目录上传脚本](market-upload.md) 从命令行计算清单并直传，无需文件夹选择框。去掉外层目录名后，.iyw-plugin.json 必须在清单根。v1 上传其合法原生清单和组件；v2 只上传转换后的 HostGateway 清单、技能、运行时和资源，不带禁止路径。packageType 明确为 plugin；两者均走用户上传接口。

## 安装后的五层准备

1. **安装**：按计划取得准确版本，校验字节、源文件树、清单摘要和组件后提交。
2. **配置**：在插件参数界面保存环境字符串；密钥由宿主安全存储，普通响应仅给configured状态。
3. **权限及开关**：授权当前项目／Agent所需权限，启用相关连接器；安装不会自动授予业务权限。
4. **业务账号**：用户明确发起login，打开验证过的地址；status确认登录，logout后再确认已退出。
5. **业务验证**：查到当前插件能力，读取完整schema，按实际授权调用；确认业务结果。

这些状态独立。未选项目、参数缺失、权限待批准、连接器关闭、依赖未准备、业务账号未登录都可能阻止工具；不能通过换命名空间、直接运行内部脚本绕过。

业务参数按当前账号、企业空间、项目和连接器隔离；保存变化后旧运行时和账号状态可能失效。已有秘密输入留空表示保留，明确clearKeys才清除；不要为“保存一次配置”反复要求用户输入原密钥。

读取准备信息后操作须带contextToken；项目、账号、版本、配置或权限变化导致旧token失效，应重新读取并重新显示当前上下文，不重放旧操作。

## 典型生命周期

| 意图 | 路线 | 完成证据 |
| --- | --- | --- |
| 首次发布 | 作者校验→uploads/init→直传→complete→回读 | 正确版本、packageType和audience |
| 公开市场发布 | global_market＋依赖全公开 | 目标用户可读并能取得安装计划 |
| 安装使用 | install-plan→ticket→校验→宿主安装→准备 | 目标项目／Agent可发现并使用能力 |
| 升级 | 新SemVer→新计划→安装→必要时重新授权 | 新版本已启用且旧结果不再覆盖 |
| 回退 | 本地版本列表→inspect→apply | 指定旧版本、依赖和准备状态真实恢复 |
| 修复旧安装 | recovery inspect→reviewToken→apply | 回执、摘要和账号范围一致 |
| 卸载 | remove inspect→apply→status | 影响范围明确，保留共享依赖，按选择处理账号数据 |
| 下架／私有化 | 修改云受众或管理员停用 | 云市场状态回读；不能等价为已卸载 |

未授权业务写入时只核对只读连接和能力发现。引用返回的能力ID和schema；不可从插件名／connector key拼接工具名。长期文件通过[交付流程](artifact-delivery.md)提供，临时签名URL不当永久文件链接。

来源：内置plugin-creator及其references/{plugin-json-spec,native-plugin-import,plugin-preparation-v2}.md；Fusion application/skill/plugin_manifest*.go；宿主commands/{skill_market,plugin_preparation,plugin_recovery}/与plugin_runtime/。
