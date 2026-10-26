# 逐页视觉与交互差距审查

> 2026-10-27 最终冻结验收完成：29套198个正式应用行为检查、5视口63处主区几何、125组页面配对，Rust/native/WASM与真实模型链路通过。队列本地提案、元Agent/Skills恢复入口及所有前批修复均闭环。当前结论与未验证边界见 [验收说明](ui-parity-acceptance.md)。以下按过程保留，旧失败、旧端口和旧“待修复/未完成”叙述不代表最终构建。

2026-10-26 当前状态：资源卡片、多 Package 隔离、元Agent 源码 Modal、添加项目 Modal、旁问无工具确认、Inspector真实计划/全局弹框/完整摘要与压缩竞态、统计会话隔离均已独立验收。最近完整回归21套139检查及63处主区几何通过。真实 rpi-web 同源资源、密码登录、一次真实模型流式回答、旧二进制不支持branch时的诚实提示已验证。当前唯一作者provider-repair接续Providers/Devices；文件/队列和最后视觉细节尚未验收，整体1:1目标未完成。

Providers本轮发现作者误覆盖settings.css，Codex已中止原writer；从最近已验收WASM的include_str常量提取10732字节精确原文件，SHA256与构建清单完全一致（1F355B5CBC687621DF154B208B73F3D5B1561924B54073FDD900BD04839E587E）。恢复文件在target/parity-recovery/settings.before-provider.css，新增样式限定作用域后仍须通过完整主题/设置回归。当前部分源码已备份target/parity-checkpoints/provider-partial/。不使用SHA不一致的旧备份替代已验收源码。

本次追加审查：文件弹窗已有桌面1100宽、手机全屏与真实文件安全预览，但标题缺项目/只读副标题、默认未选文件留大块空白、目录计数/折叠/固定说明缺失。队列虽真实API可用，但手机条目三列导致取消按钮逐字换行，与原型纵向消息卡不同。任务见 docs/rpi-files-queue-fidelity-task.md，未启动。正文以下保留历史发现，不能当作当前仍未修或已修的证明。

多Package新增验收已证实：A+B共存、只切换A版本、移除B不改A、刷新固定版本、Prompt owner隔离、指令持久化、不改用户prompt及390/320顶部4入口可用。未通过项：包内Skill重复成两行且缺开关；资源库缺绑定操作且状态按任意会话判断。后续元Agent、Inspector、项目弹窗任务已定义，尚未启动，不能算完成。

下面1/2/4/5/6的外壳已部分修复并经38组截图审查（19种状态×桌面/手机）。第3/7/8先修出卡片布局覆盖回归，新增子区域检查复现后退回修复，目前卡片堆叠、segmented、intro与旁问折叠已独立通过；原型不改。添加项目桌面仍610vs530、手机mode/路径行与footer仍偏离，见单独任务。历史原始发现保留，最终状态以验收矩阵为准。

元Agent新增4组截图已完成。手机元Agent源码编辑目前是全页面，原型是居中可滚动Modal，含固定取消/保存footer（page-review-meta-editor-390-*）；差距明显。元Agent首页默认padding用组件自身30px而未走page-view，原型桌面38px/手机26px21px；详情源码区/操作/步骤说明的层级和原型不同，需独立下一批处理。保持结构校验、不可变测试快照、显式开发会话/返回和发布ack，不能用原型JS替代。截图诊断脚本现支持 REVIEW_PAGE_FILTER 过滤，元Agent每案例独立项目目录避免“已有项目用途锁定”干扰。

本文件是Codex对正式bundle与原型的诊断记录，不是生产作者的隐式任务授权。每次只有明确启动的单一作者修改生产源码，Codex冻结后重建验收。

证据：scripts/audit-ui-pages.cjs、artifacts/ui-pages-review.json、artifacts/page-review-<state>-<1440|390>-actual/reference.png。该脚本捕获页面、真实入口、原型对应函数，不代替行为总回归。已确认以下问题不属于数据差异：

1. 页面外壳P1：Packages/Skills/扩展/设置的桌面右300列空白；原型在这些页仍有任务上下文。正式Inspector被放进workspace隐藏父层，active_page不为workspace时随父层消失，root第三列还在。应恢复通用Inspector，保留Changes/BTW隐藏和手机行为。nav页面顶部仍显示旧会话标题，原型Packages或设置/通用等面包屑随页变化。
2. 通用page-view padding：1440正式30px，原型38px；390正式22px16px，原型26px21px。resources-page自身又额外padding28px30px导致资源页双padding（Package标题x296/y184，原型x274/y159）。正式组件在page-view内去掉重复padding，不破坏独立module场景。
3. SessionResources Packages用了简陋三行表；原型是完整packageCard堆叠（说明/counts/source/获取-安装-会话配置步骤/查看内容/导出/会话绑定）。需恢复卡片信息和操作，复用现有库Card能力及真实desired绑定；不能把未安装改成已安装/已启用。session配置应显示人类可读当前会话标题，“desired revision”改成本地会话配置文案，保留真实待节点应用语义。
4. 弹框通用尺寸：原型普通dialog桌面610px、手机390时360px(left15)；正式资源modal720px、手机370px(left10)。header原型桌面69px、手机73px，padding21px23px15px；正式61/57、padding12px20px/10px14px。body原型普通桌面23px/手机20px，正式18-20/14-16。主题专用原型宽760/手机360，正式780/366；主题手机明暗与密度应并排两列而非堆叠，footer独立于滚动body，保持取消/保存/导入完整契约。
5. 手机项目文件原型全屏390x844，正式370x820留10/12边；桌面1100x780已一致。手机AddProject正式底部全宽sheet，原型360px居中dialog。Trace全屏与桌面尺寸均已一致，勿改。
6. 图标Icon函数缺phone/info/image/chevron-right，当前相关入口退化成默认方框。补原型对应SVGpaths，其他图标不必重画。
7. BTW面板实际340列对齐，但顶部公开模型/Thinking下拉被挤成几十px宽，长说明撑高到300px；原型紧凑信息+操作栏约177px。可把模型/thinking配置放在可展开“旁问配置”，默认收起，保留真实独立会话模型/工具边界与新旁问/取消/重连/历史，给内容更多空间。不能移除真实功能或伪称原生btw插件已加载。
8. 资源库card本身重阴影/description大min-height/蓝色整段技术横幅与原型平整卡片不同；应按原型卡片的padding、line-height、边框、间距/中性info-banner呈现，保留搜索功能但紧凑，不要额外高大搜索条挤开内容。原型上方当前会话/返回对话strip也需有同等入口。

后续应先做通用frame/nav/modal，再单独做resource卡片与BTW/元Agent外观，防止一次大任务漏项；每批冻结源码，Codex重建并逐图复核。所有已通过95交互与5视口63主区布局须保留。原型39份文件hash已冻结，禁止改原型迎合实现。
