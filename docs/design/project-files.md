# 项目文件与预览

> 截图与运行报告为本地产物，不随Git分发；文中路径用于定位本地文件。保留规则与复验入口见 [测试产物说明](../guides/testing-artifacts.md)。

工具栏的文件夹按钮打开独立文件浏览器，与「对话 / 变更」并列使用，不增加常驻顶部行。变更展示 Git 差异；项目文件可浏览包含未变更文件的目录与内容。

桌面在弹窗中同时展示文件树和预览，手机使用全屏目录 → 文件预览 → 返回目录。关闭后保留原会话、消息滚动位置、草稿与运行状态。每个项目单独记忆搜索、目录展开及当前文件。

支持按文件名/路径搜索、展开与折叠目录、Markdown 阅读/源码、图片、代码行号与换行、静态 HTML、复制路径和引用路径到草稿。HTML 预览不执行脚本、不加载外部资源，属于文件静态预览，不是开发服务器或完整网站运行预览。

当前原型使用 `tools/review/build-project-files.cjs` 生成的公开源码白名单快照；`rpi-work`、`pi-rust`、`rpi-package` 分别提供独立目录。并非实时文件系统，也未接入文件编辑或任意目录访问。单个文本快照最多 24 KiB，截断会明确提示。

正式版由 PC 服务端提供项目文件树和按需内容接口，返回项目 ID、规范路径、内容类型、大小、版本、截断状态。手机沿用项目授权；客户端不提交任意绝对路径。图片通过节点文件资源接口读取；大文件按范围读取；运行预览另行建模为节点服务端口与受控预览 URL。

重新生成原型快照：`node tools/review/build-project-files.cjs`。

验证：`node tests/design/verify-project-files.cjs`，覆盖目录、搜索、源码与预览、HTML 沙箱、跨项目隔离、未发送草稿与滚动位置保留、320–768px 和桌面手机预览。记录见 `artifacts/project-files-validation.json`，截图见 桌面文件预览（本地生成：`artifacts/prototype-project-files-desktop.png`）、手机目录（本地生成：`artifacts/prototype-project-files-mobile-tree.png`）、手机 HTML 预览（本地生成：`artifacts/prototype-project-files-mobile-html.png`）。
