## RPI Work v0.1.26

- 修复 Windows 从资源管理器启动 RPI Work 时出现的黑色控制台窗口，适用于 x64 和 ARM64。

## 下载安装

- macOS 下载对应架构的 ZIP，解压后打开 `RPI Work.app`。应用使用临时签名。
- Linux 下载对应架构的 tar.gz，解压后运行 `work-gpui`。构建基于 Ubuntu 24.04，运行需要支持 Vulkan 的图形驱动。
- Windows 下载对应架构的 ZIP，解压后运行 `work-gpui.exe`。

旧版客户端请下载对应平台的桌面包并手动安装。支持自动更新的 GPUI 客户端使用 `gpui-update.json` 和 `gpui-update.json.sig` 验证更新。
