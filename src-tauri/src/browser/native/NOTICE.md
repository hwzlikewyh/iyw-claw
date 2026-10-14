# 原生浏览器 Agent 脚本

`agent.bundle.js` 改编自本机 `D:/projects/codeg/browser-agent`，使用 Apache-2.0
许可证。本目录保留该许可证。脚本来自 codeg 的已构建产物，仅通过当前项目已有的
esbuild 压缩，不启动另一个 Playwright 或 Chromium 浏览器。

其中无障碍树实现源自 microsoft/playwright v1.63.0 的 injected/ariaSnapshot
及其 import 闭包，版权和 Apache-2.0 许可证声明保留在脚本和 LICENSE 中。

iyw-claw 适配层负责工具参数、页面代际、控制权和生命周期；参考项目的 MCP
companion、运行时配置及用户数据没有复制。
