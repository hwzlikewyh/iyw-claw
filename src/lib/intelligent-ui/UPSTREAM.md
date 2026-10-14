# 交互引擎来源

engine/ 基于 DEEIX-AI/DEEIX-Chat 的 packages/core/src/ui-block，
commit 5dc28abf3f75beec1c684c4f5c2e47a58d647665，Apache License 2.0。
https://github.com/DEEIX-AI/DEEIX-Chat/tree/5dc28abf3f75beec1c684c4f5c2e47a58d647665/packages/core/src/ui-block

变更：按职责拆分模块；改为 iyw-ui/iyw-widget；兼容项目 ES2020；
对属性查找、输入数量和保留状态名增加边界；tabs 默认值统一为索引；
围栏按 CommonMark 规则移除开头缩进。
保留上游著作权及 Apache-2.0 授权，完整 LICENSE、NOTICE 在本目录。
React 组件、Rust 状态服务和集成由 iyw-claw 实现。
