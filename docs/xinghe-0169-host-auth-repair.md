# 星河 0.169.0 实例认证入口修复

会话 991 在发送前连续四次报缺少 `CODEX_API_KEY`。已安装的 0.1.351 实际内核为
0.169.0；该版本新增 `request_setup::resolve_request_auth`，在读取实例 AuthManager 前
直接调用 `bearer_auth_for_provider`。旧 `resolve_provider_auth` 中的宿主适配因此被绕过。

用户已要求修复。验收：新入口在受管 provider 且 manager 持有 ExternalAuth 时优先读取
实例凭据，返回完整 credential 和 Bearer/token 头；认证所有者变化仍由既有请求准备检查
处理；其他 provider 继续遵循配置的环境认证。没有可用宿主凭据时明确失败，不发送匿名请求。

修复仅接通现有宿主认证，不修改全局环境、auth.json、用户会话或密钥存储。
推理、目录刷新、启动预热和辅助请求共用新入口；重连创建的实例使用新凭据。

验证完成：`cargo check --manifest-path harness/codex/Cargo.toml --features bundled-host
--lib --locked --offline --jobs 4` 通过，含新认证入口、核心与 App Server；修改模块的
rustfmt/语法、差异检查及目录刷新/推理/辅助请求的调用链审查通过。仅报告既有未使用代码
和补丁警告。遵循仓库规则未新增或运行测试，未构建或替换本机桌面程序。
源码修复需后续应用构建发布才会生效。用户此前已授权提交、推送和合入主分支。
