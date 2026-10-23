# RPI Work

Rust Agent 工作台的功能设计、交互原型与架构讨论稿。当前阶段：**设计验证，尚未接入模型或实现 Agent 桌面程序**。

- [打开 HTML 原型](prototype/index.html)
- [功能与交互设计](docs/product-design.md)
- [框架与复用方案：Dioxus / rpi-web / rpi](docs/architecture.md)
- [工具、队列、Provider、上下文、用量与附件设计](docs/runtime-interactions.md)
- [单一对话与多文件审阅（最新交互）](docs/conversation-and-review.md)
- [Skills、ask_user、BTW 与常见交互工具](docs/skills-and-interactive-tools.md)
- [流式消息、工具输出、停止与断线恢复](docs/streaming.md)
- [会话顶部资源与可下载 Package](docs/session-packages.md)
- [整体主题抽象与切换](docs/themes.md)
- [手机扫码 / 高级手工连接 PC](docs/mobile-connection.md)
- [验证记录](docs/prototype-validation.md)

## 本地预览

`prototype/index.html` 可以直接用浏览器打开，无第三方字体、脚本或 CDN 依赖。也可以在仓库根目录运行：

```powershell
python -m http.server 4173 --bind 127.0.0.1
```

然后打开 <http://127.0.0.1:4173/prototype/>。

右上角主题按钮或个人设置可切换「纸与苔绿 / 海湾蓝 / 石墨紫」，分别选择浅色、深色或跟随系统，以及标准/紧凑密度。支持即时预览、取消恢复、保存偏好和主题 JSON 导入导出；会话、工具、Package 与变更共享同一套主题变量。

可体验：项目/会话、对话、diff、计划、工具调用详情/失败重试/子任务取消、消息队列、Provider 设置、上下文压缩、token/缓存/费用/速度统计、图片与文档附件、扩展、节点配对、主题和手机布局。

任务只有「对话 / 变更」两个标签：计划、工具与命令输出都在对话内。变更页提供 120 文件样例、目录/搜索/类型/未审阅筛选、本轮范围、单文件 diff 和移动端列表/详情。

会话顶部统一展示具名 Package、固定版本，以及本会话 Skills、Tools、提示词配置；输入框保留 Skills 快捷选择。Packages 资源库支持导出包含正文的 JSON 设计包、重新导入、模拟安装与显式启用；安装更新不会自动替换会话版本。每轮消息保存配置快照。

输入区的「常用能力」可体验 ask_user 单选/多选/自由回答、BTW 旁问、todo 和 goal 状态。侧栏「技能库」展示示例目录、来源与全文载入证据；BTW 使用独立旁路面板，显式引用才进入主草稿。

正常发送即逐段显示回复与工具输出，BTW 也支持流式回答。点击标题旁「流式演示」或输入 `/stream`，展开对话内的「流式演示控制」，可模拟断线恢复、快照重建、停止、失败与等待最终结算。阅读历史时保持滚动位置，点击「新进展」继续跟随。

「执行节点」展示 PC 服务端 / 手机客户端：PC 生成二维码，手机可用相机入口、二维码图片或链接识别；高级模式可手工填写服务地址和配对码。两种模式均经过电脑确认和项目授权，并可体验过期、拒绝、撤销和断线重连。QR 与本地图片解码可用，实际配对和同步仍是模拟，没有开放电脑网络端口。

原型不主动读取项目文件、不执行 shell、不收集密钥，也不连接设备或模型。用户主动选择/粘贴/拖入附件后做本地预览；主动打开相机或选择二维码图片后在本机识别，不上传图像。主题偏好保存在浏览器本地，任务、附件和配对状态刷新后重置。页面中的代码、测试结果、时间、设备与统计都是交互样例。

## 当前推荐（待共同确认）

以 **Dioxus 客户端 + 现有 rpi-web Rust 服务 + rpi RPC 宿主** 为优先验证方案。手机先通过响应式 Web 使用电脑/服务器上的 Agent，再验证 Dioxus Android/iOS 包装。Tauri + 现有 React 前端保留为更快复用 UI 的备选。

此仓库只增加设计材料和静态原型，未修改相邻 `pi-rust`、`rpi-package` 仓库。
