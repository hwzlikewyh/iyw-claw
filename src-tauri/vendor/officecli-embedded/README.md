# 内置 OfficeCLI-rust 命令解释器

来源：RainLib/OfficeCLI-rust v0.1.31，提交7916bb354aaa9b25cb7c3d5270a6a2f441d84878，Apache-2.0。

上游只有二进制crate，没有公开CLI库。本目录保留其命令层、schema与技能作为可链接库，文档处理crate继续从固定git提交依赖。未复制测试目录与上游构建产物。

本地修改：main入口改为显式参数的run；公开bundled_skill；所有自身重启路径添加宿主的--embedded-office前缀；禁用独立二进制安装命令；build.rs资源根改为当前crate。应用启动前分派办公命令，不启动GUI或服务端，不连接业务后台。普通预览则直接调用HCD库，不执行命令入口。

上游既有源码/函数长度按第三方代码维护，本地入口放在宿主office_embedded.rs。升级时需核对所有current_exe调用、命令协议、技能、schema与许可，禁止自动更新。
