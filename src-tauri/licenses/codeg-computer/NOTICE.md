# Computer Use 来源

本目录的执行、权限、目标引用、helper 与用户界面生产代码移植自
spacering-net/codeg v0.34.0，commit
`592131c72e2a05478e7f8398a91cb653c8e3996f`，按 Apache-2.0 使用。
原许可证见 LICENSE。适配主进程 HTTP MCP、Fusion 受管环境、双运行模式、
发布打包、品牌和会话操作记录；上游自动化测试未导入。

执行驱动为 trycua/cua 的 `cua-driver-rs-v0.32.0`，按 MIT 使用，
许可证见 CUA-LICENSE。官方归档与执行文件摘要固定在 Computer Use 源码中。
macOS 还核对调用方签名、启动要求和运行中的驱动映像。
