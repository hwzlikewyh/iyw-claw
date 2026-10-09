# 电脑操作集成主程序

用户明确要求桌面端各平台统一合并，避免电脑 helper 漏打包、版本不匹配和额外签名步骤。

## 设计与边界

桌面主程序通过 `--internal-computer-helper` 在 GUI、单实例、数据库和智能体初始化前
分流。执行器仍在独立后台进程运行，复用当前主程序文件、现有帧协议、版本握手、
驱动摘要校验、窗口授权和停止流程，不新增平行状态或工具契约。

Windows 使用隐藏进程与管道；macOS 使用现有 socketpair 且不解除主应用责任归属。
macOS 系统权限归主应用，设置引导授权原助理。主应用签名用于双方身份检查；
ad-hoc 包可运行，更新重签后可能需要用户重新授权，实际行为须在 Mac 验证。
授权不再构成对主应用启动的脚本的 OS 隔离；工具层仍检查共享范围与操作权限。
用户已明确选择这一边界，不静默声称共享权限能约束所有 shell/API。

既有服务器发布契约仍含独立执行器；本轮修改桌面打包和执行入口，保持服务器更新
与旧发布包兼容。独立执行器复用抽出的权限请求模块，避免复制平台调用代码。
Fusion 管理的 cua-driver 仍为外部受管工具；本轮不改变工具版本、摘要或下载来源。

## 验收与步骤

1. 已实施：主程序早期入口及启动路径 -> 确认不会创建第二个 GUI，stdin/stdout
   只承载电脑协议，后台退出/停止与旧调用链一致。
2. 已实施：桌面发布移除独立 helper -> 校验根/平台配置、staging、签名、安装校验
   改为主程序内部身份检查，服务器旧契约不受影响。
3. 已实施：macOS 权限身份与文案 -> 系统授权请求、Finder 定位、错误提示、身份
   检查均指向主应用，保留固定驱动验证。
4. 已验证：静态调用链、Rust 格式、TypeScript noEmit、定向 ESLint、JSON/脚本语法
   和发布配置一致性通过。Cargo metadata locked/offline 验证功能依赖与 bin 门禁。

桌面默认启用 computer-executor；独立 bin 仍需 computer-helper，避免桌面构建顺带
生成旧 helper。Linux X11 的条件编译统一使用执行器功能，服务器旧功能仍包含它。
本轮没有修改窗口授权、取消计数、后台通信协议或驱动固定摘要。

按仓库规则不在本机编译/启动桌面端，不新增或运行自动化测试。
正式安装包、Mac 权限弹窗及窗口读取/点击/输入仍需要 CI 和实机验证。

Apple 官方参考：

- https://support.apple.com/guide/mac-help/allow-accessibility-apps-to-access-your-mac-mh43185/mac
- https://developer.apple.com/documentation/coregraphics/cgrequestscreencaptureaccess()

Jina 读取失败后回退 Apple 官方页面与文档 JSON，沿用仓库现有 API 和动态加载方式。
本轮没有引入新的第三方 API。
