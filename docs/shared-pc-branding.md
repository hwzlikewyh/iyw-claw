# 安装配置驱动的 PC 品牌

所有品牌使用同一个 `iyw-claw` PC 包。安装器保存名称、PNG Logo、品牌 ID、候选发布 ID 和晋级通道；应用启动时读取配置，构建期不修改包的签名身份。

Windows/Linux 配置在安装根目录 `config/branding.json`，macOS 配置在 `.app` 旁的同名 `.app.branding/branding.json`。不修改签名应用内部的 Info.plist、图标或主程序。

运行时品牌负责界面、标题、图标、协议、单实例、自动启动、本地文件与凭据隔离。原助理保留原目录；其他品牌使用 `.iyw-claw-<brandId>`。配置错误阻止启动并写入诊断日志。

更新查询始终使用 `product=iyw-claw`。通用 Windows 向导须声明 `runtime_branding=1`，在更新或卸载前读取目标品牌。旧向导不具备这一能力时，应用在交接前报错，避免覆盖品牌配置。

候选使用最终 SemVer，后台发布仍按 beta/stable 区分。PC 通过现有版本查询识别候选晋级，保留同一发布的下载缓存；用户手动选择的通道优先。候选文件未发布前不切换线上稳定版。

系统签名身份保持 `app.iywclaw`。固定签名的系统分享入口属于原助理；其他品牌使用独立的手动导入收件箱。macOS Finder 中签名包的内置图标和系统身份仍来自公共包，运行中的 Dock 与窗口使用安装配置。

本机构建、打包和启动受 `AGENT.md` 禁止。验证使用语法解析、定向 lint 和远端签名 CI。CI 中的 NSIS 安装检查与依赖线上环境绑定的完整安装检查分别报告，未登记候选环境绑定时不宣称完整安装验收通过。

完整安装器配置及三端构建入口见相邻 `iyw-claw-installer/docs/shared-pc-branding.md`。
