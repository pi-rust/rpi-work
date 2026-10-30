# rpi-work-extension-demo

真实 rpi 原生动态库 Demo，只提供进程内计数工具 `work_demo_counter`（1–10 步、partial 进度）和只读命令 `work-demo-status`，不读写业务文件或访问网络。

独立 Cargo workspace，使用 crates.io 的 `rpi-plugin-sdk =0.8.3`，可直接构建：

```shell
cargo test --manifest-path examples/extension-demo/Cargo.toml --locked
cargo clippy --manifest-path examples/extension-demo/Cargo.toml --locked --all-targets -- -D warnings
node tools/dev/extension-demo.mjs --verify
```

以上命令在仓库根目录运行。runner 会在 `target/` 创建隔离项目，通过真实 `rpi package link` 安装并加载，使用本地固定模型 fixture 驱动真实工具，最终关闭宿主并卸载。不修改用户全局插件、认证或会话目录。

实时 Dioxus Demo、自动浏览器验证和已验证/未实现边界见 [使用指南](../../docs/guides/extension-demo.md)。这是可执行原生插件源码，不是资源库 `extension.design.json` 设计包，也不是正式节点插件管理或 UI 协商协议。
