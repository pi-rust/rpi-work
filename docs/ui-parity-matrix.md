# 1:1 还原验收矩阵

2026-10-27，原型还原验收完成；最终结论与平台/协议边界见 [验收说明](ui-parity-acceptance.md)。基准为只读 `prototype/`，正式实现为 Rust / Dioxus。rpi CLI 负责生产代码，Codex 负责独立审查、冻结构建和验收；不以作者自验代替正式浏览器验证。

最新已验收 checkpoint：`queue-proposals-frozen`。29套198个行为检查、5视口63处主布局/排版对照通过；25页面状态 × 5视口 = 125组配对截图，无捕获错误。最终源码、release WASM与全部报告SHA一致；完成审计见 `artifacts/ui-completion-audit.json`。

| 原始范围 | 已验证的结果 | 证据/实际边界 |
| --- | --- | --- |
| 主工作区与移动导航 | 紧凑52+44顶部、桌面三列、手机抽屉与会话切换、5视口无横溢出 | layout-theme、geometry、shell-details |
| 对话、工具、计划、Ask、流式 | 同一时间线、公开工具IO、稳定ID、历史/实时去重、单选/多选/自定义回答、多pending隔离、停止与真实settlement | conversation、streaming、inspector；不展示私人推理 |
| 项目与会话 | 已有/新建目录、目录浏览、自定义别名、用途仅添加时选择、重试不重复创建、会话管理 | project-modal、project-alias、behavior |
| 输入与草稿 | IME/Enter、模型/thinking/实际Tools、会话/节点草稿隔离、失败不丢、阅读历史不跳 | behavior、auth-scope、btw-scroll |
| 队列 | 服务端真实顺序与状态、steer/followUp、取消/清空、accepted与不确定响应、迟到请求不能清除新草稿 | queue-dialog、queue-proposals已通过；本地暂停/排序/编辑、草稿保护、跨会话恢复与显式入队均验证，不冒充节点能力 |
| BTW | 真实独立普通session/SSE、必须回读activeTools明确为空才发送、引用/取消/重连/历史 | btw-tools、btw-scroll；不是声称启用原生BTW扩展 |
| 图片与附件 | 安全栅格/文本预览、删除、大小限制、显式文本引用、失败保留 | conversation、files；当前图片未上传、不声称模型已看到 |
| 文件与多文件变更 | 懒加载内联树、路径校验、搜索/折叠/重试、README、代码/Markdown/沙盒HTML、长文独立滚动、120文件筛选与diff | file-tree、files、shell-details、navigation-trace |
| 用量/footer | 输入含缓存读写、输出、缓存命中率、费用、未知值、累计与上一轮区分、真实context window | runtime-stats；未知显示—，不伪造速度或历史计时 |
| 上下文与压缩 | 真实手动compact与结算、公开branch摘要、竞态/跨会话隔离；auto开关与range为明确本地提案 | runtime-stats、inspector、final-controls；本地auto提案不写节点，不推断已启用 |
| Trace | 会话/轮次/span、客户端真实耗时、脱敏IO、手机弹框、未知历史 | navigation-trace；不冒充Provider服务端耗时 |
| 主题 | 四主题含Reddit、深浅/系统、密度、自定义完整JSON、严格导入/导出、取消/持久化 | custom-theme、layout-theme；新增UI使用语义tokens |
| 设置/Provider | 设置内通用/节点/Provider；真实模型/Key命令、认证来源/能力/未知值、重启提示、可见连接入口 | provider-cards、settings、final-controls；不把本地凭据就绪称远端认证成功 |
| PC/手机连接 | PC地址二维码实际编码/解码、手机扫描/图片与手工输入、密码认证、迟到相机清理 | settings、real-web-assets；二维码是版本化地址，不虚构配对授权 |
| Packages/Skills/扩展 | 完整manifest/源码/未知字段、原子冲突拒绝、多个Package、同ID固定版本切换、owner隔离、下载/节点安装/会话选择区分 | multi-package、resources-meta；没有节点安装API时完整本地导入/导出/配置与恢复 |
| 会话顶部配置与Skills快捷选择 | 四入口、Package固定版本、真实Tools回读、Skills开关、Prompt/指令草稿；未选Skill可打开会话配置恢复，再显式引用 | resources-meta、multi-package、authoring-context；不私改普通用户prompt |
| 元Agent | 按项目用途入口、个人源码、编辑/差异/取消/超限保护、开发会话完整快照、真实关联Inspector、版本快照/发布回执 | meta-editor、meta-feedback、resources-meta、authoring-context；结构校验不冒充源码执行 |
| 模态框与生命周期 | 统一尺寸/焦点/Escape、嵌套不误关父框、根层唯一queue/stats/context、scope与drop清理 | modal-lifecycle、shell-details、meta-feedback |
| 工程与真实集成 | locked workspace tests、fmt、native/WASM strict Clippy、native UI/CLI与release WASM构建；实际rpi-web同源assets/API/auth与模型流式completed | engineering-validation、real-web-assets-validation、real-model-validation、real-context-summary-validation |

最终脚本：`node scripts/verify-ui-suite.cjs`、`$env:REVIEW_ALL_VIEWPORTS='1'; node scripts/audit-ui-pages.cjs`、`./scripts/verify-rust-workspace.ps1`。报告都在 `artifacts/`；suite同时验证源码与原型哈希，截图清单不是像素差异率或功能成功的替代品。

真实验证节点只使用本任务持有的43340/43341隔离实例；owner/PID/config见 `target/parity-real-{node,web}-v2/owner.json`。3030/3031已属其他进程，未修改、未终止、未向其尝试凭据。43341有测试专用密码与独立registry/session目录。

当前上游边界：已安装rpi 0.9.2虽声明branch能力，实际返回unknown command，界面提供更新/重试；自动压缩配置写入、原生Package安装、逐设备ACL没有对应可用接口。rpi-http确有20MiB项目文件上传路由，但当前UI尚未接入，原生RPC prompt仍只接收字符串，不能宣称支持模型视觉输入。移动原生包、手机真机摄像头和原生WebView真机操作未验证；已验证响应式Web及native编译。

完成审计必须逐项覆盖上表与原始还原目标，不能把“测试全绿”或“数据不同”用于豁免缺失布局、控件、交互。队列提案及最终源码回归已完成，未发现待修复的必需UI/交互项；上游与硬件边界仍按上文明确保留。
