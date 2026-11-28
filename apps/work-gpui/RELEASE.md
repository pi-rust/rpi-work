## RPI Work v0.1.25

- 新增 rpi Host 管理，可查看本机持久化 Host area 和 Agent 状态，在 macOS 终端进入 Host 或连接已有 Agent。
- 新增飞书和企业微信机器人配置、可控的功能入口，以及支持斜杠提示词模板的桌面插件。
- 会话可在重启后恢复草稿、阅读位置和工具详情展开状态。
- 改进附件粘贴和预览、项目导航、设置搜索、本机 rpi 配置和节点连接体验。
- 原生桌面包提供签名更新清单，支持在应用中检查、下载并确认重启安装。

## 下载安装

- macOS 下载对应架构的 ZIP，解压后打开 `RPI Work.app`。应用使用临时签名。
- Linux 下载对应架构的 tar.gz，解压后运行 `work-gpui`。构建基于 Ubuntu 24.04，运行需要支持 Vulkan 的图形驱动。
- Windows 下载对应架构的 ZIP，解压后运行 `work-gpui.exe`。

旧版客户端请下载对应平台的桌面包并手动安装。支持自动更新的 GPUI 客户端使用 `gpui-update.json` 和 `gpui-update.json.sig` 验证更新。
