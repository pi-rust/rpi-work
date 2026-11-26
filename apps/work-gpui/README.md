# RPI Work · GPUI 工作台

当前唯一维护的 UI 入口。布局参照 [`design/reui-message-layout/`](../../design/reui-message-layout/)，由 GPUI 原生渲染。

在仓库根目录运行：

```bash
# 原生布局样例，窗口标注「示例 · 未连接」
task desktop

# 认证后连接已运行的 Work 节点
# 如需登录，在环境或 .env.local 中设置 RPI_WORK_PASSWORD
task desktop -- --connect http://127.0.0.1:13031

# 无 Task 时可用 Cargo 直接启动
cargo run --manifest-path apps/work-gpui/Cargo.toml
```

项目和会话使用 `work-client`。选中的会话由共享 `SessionConnection` 持续接收消息、工具与运行结算，断线后恢复快照和订阅。节点声明发送能力且会话空闲时，按 Enter 发送纯文本，Shift+Enter 换行。草稿按会话保存在当前窗口。节点受理后等待运行结算。发送结果未知时，有回执引用可点「核对回执」查询，草稿保留且客户端不会重发。

输入区支持原生输入法、多行编辑、鼠标及键盘选区、系统剪贴板。输入法候选尚未确认时，Enter 交给输入法。

输入栏的模型和思考等级由当前会话声明的能力、运行状态和节点目录决定。同名模型按供应商区分。修改受理后读取节点当前值，存在可核对的受理结果或回执时可点「核对配置」，只读取状态。

运行期间可点击方形按钮请求停止，节点结算后恢复发送。节点发来的单选审批会显示在输入框上方，选项按节点提供的值提交，等待节点关闭审批。停止或回复结果未知时，可以用节点返回的回执引用只读核对。

历史回答和流式回复通过共享 Markdown 视图显示标题、强调、清单、引用、表格和语法着色的代码块。代码块支持复制和独立滚动。通过共享校验的 HTTP(S) 链接可在系统浏览器打开，图片显示替代文字。

左栏的「新对话」在当前节点创建会话并读回详情。窗口顶栏的前进和后退按访问顺序恢复项目和会话。窄窗使用覆盖式导航和审阅抽屉，桌面可拖动右栏分隔线。

右栏的「文件」通过当前节点读取所选项目的目录和源码，支持分页、返回、刷新和复制原文。「变更」在同栏显示工作树文件列表和选中文件的差异。可分别查看已暂存和未暂存范围，并复制差异原文。文本差异的行末加号可添加批注。提交批注前会重新读取同一文件和暂存范围，核对补丁内容。核对成功的批注会出现在该行和当前会话草稿中，可随下一条消息发送。「运行审阅」核对工作树仍有改动后，在当前会话提交审阅请求，并保留已有草稿。

macOS 右栏的「＋」打开内嵌浏览器标签。输入 HTTP(S) 地址后可前进、后退、刷新、在系统浏览器打开，以及切换自适应和 390 点移动视口。标签切换和面板隐藏保留网页状态，每个标签使用非持久化 WebKit 数据存储。

附件、复杂审批表单、网页批注和独立审阅会话仍在迁移。

## 构建与平台

GPUI 0.2.2 与旧 Dioxus 的 `cocoa` 版本要求冲突，本应用使用独立 Cargo workspace。macOS 通过 `runtime_shaders` 在运行时编译着色器，可使用 Command Line Tools。Linux 和 Windows 的 GPUI 构建尚待验证。GPUI 0.2.2 的平台后端面向桌面。

```bash
cargo fmt --manifest-path apps/work-gpui/Cargo.toml -- --check
cargo test --manifest-path apps/work-gpui/Cargo.toml
cargo clippy --manifest-path apps/work-gpui/Cargo.toml --all-targets -- -D warnings
```
