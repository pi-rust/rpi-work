# HTML 原型验证记录

2026-10-23，Windows，Chrome 146.0.7680.178，Playwright 无头浏览器。本机未提供 Chrome DevTools MCP，因此使用实际 Chrome + Playwright 验证。

## 已执行

`node --check prototype/app.js` 通过。

`node scripts/verify-prototype.cjs` 通过以下交互与布局检查：

- 单次允许与拒绝产生不同演示状态；示例执行结果、计划同步。
- 三个文件的 diff 可切换。
- 创建任务校验、发送、停止及模拟回复。
- 用户输入作为纯文本渲染，HTML 不被当作 DOM 执行。
- 模型/模式、会话搜索、扩展开关。
- 设备配对与撤销的模拟流程。
- 浅色/深色设置。
- 375、390、768、1024、1440px 视口无文档横向溢出。
- 手机导航与上下文抽屉，关闭侧栏完全移出视口。
- 桌面手机预览切换。
- 直接 `file://` 打开，无需 dev server。
- 没有观察到 JavaScript、console 或资源加载错误。

已人工查看以下浏览器截图：

- [桌面](../artifacts/prototype-desktop.png)
- [手机](../artifacts/prototype-mobile.png)

机器可读结果：[prototype-validation.json](../artifacts/prototype-validation.json)。

## v0.2 运行能力补充

`node scripts/verify-runtime.cjs` 额外验证：

- 本轮/会话统计、缓存命中计算、避免重复计入缓存和思考 token。
- 忙碌时禁止手动压缩、自动压缩开关、摘要预览、压缩后保留累计用量。
- 工具参数/输出、筛选、失败重试与子任务取消模拟。
- 队列排序、编辑、暂停/恢复、模拟交付和清空。
- 本地附件选择、图片缩放、不支持类型拒绝、文本转义、发送后预览保留。
- 运行中加入队列、斜杠命令路由。
- Provider 设置及成功/429 错误模拟。
- 新会话队列/附件/用量隔离，375–1440px 布局。
- 没有观察到外部上传/模型连接请求或 JavaScript 错误。

机器记录：[runtime-validation.json](../artifacts/runtime-validation.json)。已查看 [工具详情](../artifacts/prototype-tools.png)、[手机统计](../artifacts/prototype-stats-mobile.png) 和 [手机上下文](../artifacts/prototype-context-mobile.png) 截图。

## v0.3 单一对话与多文件审阅

原工具/计划/执行记录独立标签已删除，已有验证脚本同步改为检查内联内容。

`node scripts/verify-review.cjs` 验证：

- 只保留对话与变更；8 个工具、计划、命令输出都在同一时间线。
- 120 个文件按目录分组，页面仅渲染当前选中文件的 diff。
- 路径搜索、空结果、修改/新增/删除/重命名筛选与二进制占位。
- 本轮 3 文件范围、已审阅/未审阅、下一个未审阅、上下方向键、目录折叠。
- 针对文件提意见回到同一会话。
- 375–1440px 无文档横向溢出，手机列表/详情/返回流程。
- 没有观察到 JavaScript 或 console 错误。

记录：[review-validation.json](../artifacts/review-validation.json)。已查看 [桌面多文件](../artifacts/prototype-changes-desktop.png)、[手机文件列表](../artifacts/prototype-changes-mobile-list.png)、[手机 diff](../artifacts/prototype-changes-mobile-diff.png) 和 [内联工具](../artifacts/prototype-conversation-inline.png)。

## v0.4 Skills 与交互工具

`node scripts/verify-capabilities.cjs` 检查：

- ask_user 单选/多选/自由回答及安全文本渲染；默认不选择、不自动提交。
- 取消、超时、其他设备处理后的只读结果；不捏造用户答案。
- 技能搜索、来源、同名覆盖、解析失败、手动指定、原文与使用记录。
- 技能草稿与排队引用不提前标记载入；模拟发送/交付之后产生记录。
- todo 返回对话进度；goal 仅展示样例，不创建真实目标。
- BTW 与主任务的队列、取消、历史隔离；隐藏可继续，显式引用保留原草稿且不发送。
- 会话切换清理旁问并隔离主草稿，防止旧回调更新新会话。
- 375–1440px 布局，以及没有外部上传/插件/推理请求。

当前默认时间线包含 11 条样例调用，包括技能 read、历史 ask_user 和 todo。旧稿中的 8 条是此前版本的样例数量。

记录：[capabilities-validation.json](../artifacts/capabilities-validation.json)。截图：[Skills 目录](../artifacts/prototype-skills-desktop.png)、[手机提问](../artifacts/prototype-ask-mobile.png)、[手机旁问](../artifacts/prototype-btw-mobile.png)。

## 流式消息增补

`node scripts/verify-streaming.cjs` 验证主会话/BTW 逐段回复、工具快照覆盖与日志追加、草稿节点保留、重连去重、模拟快照重建、agent_end 等待 agent_settled、停止确认、失败与部分内容保留、旧回调清理、滚动锚点和 375–1440px 布局。未产生外部模型、工具或上传请求。

回归脚本对发送/停止的等待改为等待运行结算；已出现第一段正文不再等同于回复完成。

记录：[streaming-validation.json](../artifacts/streaming-validation.json)。截图：[桌面流式消息](../artifacts/prototype-streaming-desktop.png)、[手机流式消息](../artifacts/prototype-streaming-mobile.png)。这验证的是本地事件模拟器，真实 SSE 和 Dioxus 尚未接入。

## 会话 Package 增补

`node scripts/verify-packages.cjs` 验证顶部资源条、会话级 Skills/Tools 开关、提示词保存和套用、真实 JSON 文件下载与导入、模拟安装和启用分离、更新版本固定、每轮快照、运行配置锁定、跨会话隔离与 375–1440px 布局。原有五组浏览器检查继续通过。

下载文件包含示例资源正文，可以重新导入；没有下载远程插件、连接模型或真正安装节点扩展。结果与截图见 [会话 Package 设计](session-packages.md)。

## 复现

功能语义审阅回归：`node scripts/verify-runtime-semantics.cjs`。验证原生 aborted/suspended/compacted/declined 等结算结果、pendingResume、模型/thinking/模式按会话隔离、follow-up 文案和权限 gate/规则存储区别。源码基线和未接通项见 [功能审阅](upstream-functional-review.md)。主题回归已扩展到 Reddit 橙，共四套风格、八种浅深色组合。

连接验证：`node scripts/verify-connections.cjs`。覆盖独立解码实际 QR 截图、二维码图片识别、相机拒绝回退和模拟媒体轨道释放、手工地址/配对码、PC 确认与权限范围、邀请刷新失效、撤销、重连、Agent 任务隔离和 375–1440px 布局。没有接入真实配对 API 或跨设备通信；相机真机扫描尚未验证。记录见 [手机连接 PC](mobile-connection.md)。

主题系统验证：`node scripts/verify-themes.cjs`。覆盖三套主题的浅深色布局、主要文本对比度、预览恢复、系统模式、偏好迁移、JSON 导入导出与状态保留；原有六组交互脚本继续通过。记录及截图见 [主题系统](themes.md)。

一个终端运行 `python -m http.server 4173 --bind 127.0.0.1`，另一个运行 `node scripts/verify-prototype.cjs`。

脚本会优先查找本项目或相邻 rpi-web 前端已安装的 Playwright。默认 Chrome 路径对应本机 Scoop 安装；换机器可通过 `CHROME_PATH` 指定浏览器，通过 `PROTOTYPE_URL` 指定原型地址。

## 移动端快速切换会话

`node scripts/verify-session-switcher.cjs` 验证手机顶部会话入口、跨项目选择、最近访问排序、草稿恢复、搜索空状态、新建会话、Escape 关闭和焦点恢复。320–430px 窄屏的长名称不横向溢出，会话列表较长时独立纵向滚动；桌面的手机预览使用同一入口。正式桌面保留原面包屑和侧栏。

原型沿用当前演示的运行限制：生成中需先停止或完成，才能切换或新建会话。机器结果见 [session-switcher-validation.json](../artifacts/session-switcher-validation.json)，截图见 [移动端首页](../artifacts/prototype-session-switcher-mobile-home.png) 和 [会话列表](../artifacts/prototype-session-switcher-mobile.png)。

会话空间补充：`verify-prototype.cjs` 在 375–1440px 验证导航与配置总高度不超过 100px；`verify-packages.cjs` 验证统一配置入口和手机能力分段切换。桌面与手机截图见 [桌面](../artifacts/prototype-conversation-space-desktop.png) 和 [手机](../artifacts/prototype-conversation-space-mobile.png)。

项目文件补充：`node scripts/verify-project-files.cjs` 验证按需打开的目录与预览、各类文件、项目隔离、引用到草稿及手机返回路径，详见 [项目文件与预览](project-files.md)。顶部仍保持两行，不增加常驻文件区。

设置导航补充：执行节点和 Providers 从侧边栏移至设置。原型交互、运行能力、连接配对及主题验证脚本已改为通过新入口验证。截图见 [桌面设置](../artifacts/prototype-settings-desktop.png)、[手机设置](../artifacts/prototype-settings-mobile.png)。

追踪补充：`node scripts/verify-traces.cjs` 验证回合与步骤的分层瀑布图、耗时、失败详情、流式与结算状态、重放去重、会话隔离及手机布局，见 [会话追踪与耗时](session-tracing.md)。

资源生命周期补充：`node scripts/verify-resource-lifecycle.cjs` 验证扩展、独立 Skills、Package 的下载/安装/启用前置与获取目标区分，见 [资源下载前置](resource-acquisition.md)。

元Agent 补充：类型选择位于「添加项目」表单，创建后与普通项目并列。`node scripts/verify-meta-agent.cjs` 覆盖两类项目创建、源码、校验、测试快照、个人发布与资源获取，以及 320–1440px 和手机预览。详见 [元Agent](meta-agent.md)。

项目打开补充：`node scripts/verify-project-opening.cjs` 验证打开已有目录、自定义显示名称、重复目录复用、新建目录模式、元Agent用途，以及目录选择和手机布局，见 [项目打开](project-opening.md)。

## 未验证范围

没有连接模型、没有运行真实 cargo 测试或 Agent 工具，没有修改相邻仓库。没有验证 Dioxus、WASM 构建、原生手机、WebView、SSE 服务接入、网络鉴权或 PWA。浏览器窄视口验证不能等同于真机与软键盘验证。
