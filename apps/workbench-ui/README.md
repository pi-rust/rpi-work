# RPI Work · 已确认工作台实现（整体迁移未完成）

正式源码与Web构建入口在本目录，不依赖 `design/`；视觉/交互基准仍是已确认的 `design/reui-message-layout/`。前端复用 `work-client-wasm` → `WorkspaceClient` / typed NodeApi，以及 `work-viewmodel-wasm` / RunProjection；不在TypeScript写HTTP endpoint、认证协议、执行状态机或第二个owner。

当前已迁入整窗外壳、独立项目/置顶/最近、真实Web浏览器和项目登记/会话创建。**源码入口迁出不代表完整1:1、Native/Android、权限/结算或正式安装发布已完成**；旧 `apps/work-ui` 未改动。范围与边界见[迁移清单](../../docs/design/confirmed-workbench-migration.md)。

## 构建与启动

在仓库根目录安装浏览器测试依赖 `npm ci --prefix tools`，然后：

```bash
cd apps/workbench-ui
npm ci
npm run build             # Rust WASM → tsc → 产品 dist/
npm run dev              # 默认无本机节点代理，无内置样例
npm run preview          # 预览产品 dist/
```

开发连接自己的节点时，显式设置 `RPI_NODE_URL=http://127.0.0.1:<port>` 再运行dev/preview，并用 `?live=1` 连接同源代理；不默认探测或连接13031。真正由节点托管时，公开 `/.well-known/rpi-work-entry` 指示自动连接当前origin，认证仍走Rust客户端；不采用元数据里的远端跳转地址。

产品默认从零项目/零会话/未知指标启动。普通静态托管、断开、认证错误不会回落样例，运行观察缺失不填充样例模型/思考/成本/上下文/步骤。未连接时可以保留草稿，但发送、队列与创建不冒充可用。

### 显式隔离的测试样例

```bash
npm run build:fixture     # 独立 dist-fixture/，不覆盖 dist/
npm run preview:fixture  # 必须访问 /?fixture=1 才启用样例
# 或 npm run dev:fixture，再访问 /?fixture=1
```

产品Vite alias只导入空初始快照；只有fixture构建才导入 `tests/fixtures/workspace.ts`。URL参数不能为产品构建启用样例；fixture构建也不会默认启用。示例页面标注“示例 · 未连接”，不能发送消息或登记真实数据；显式连接后清除样例，断开后也不自动恢复样例。样例不进入产品模块图或 `dist/`。

`src/lib/types.ts` 是渲染数据类型，Rust仍拥有排序、工具目标、指标格式和主题规则。`src/pkg/`、`dist/`、`dist-fixture/` 是忽略的构建产物，不提交或用设计目录作运行依赖。

## 验收

```bash
npm run build
npm run build:fixture
npm run verify:all        # 自起独立fixture预览，按序跑17组，结束仅清自身进程
npm run lint
```

| 脚本 | 覆盖 |
| --- | --- |
| `verify:entry` | 产品无样例、独立测试构建显式启用、认证失败/缺失观察、显示WASM迟到、ACK不清后来编辑及同地址重连，8项 |
| `verify:session` | 精确会话权限、ACK/结算、重连终态、abort、真实选项、配置回读、并发观察、迟到diff，以及丢ACK的只读回执恢复/明确拒绝/无证据锁定/跨重连GET/queue HostResponse，23项 |
| `verify:scopes` | 同地址重连后的迟到Provider目录/写回执/QR成功与错误，A→B→A后取消审阅，旧install finally不得干扰新请求，6项 |
| `verify:reading` | 阅读/经典几何与偏好、明暗/紧凑/390px、草稿选区/工具/真实 iframe 保持、canonical 结算与历史恢复、长段落锚点、底部跟随、会话和节点隔离，以及刷新时的历史等待、稳定锚点、像素回退和存储拒绝，16 项 |
| `verify:rich` | 共享Rust Markdown/代码token、安全HTML/链接/惰性图片、精确复制及失败/迟到隔离、流式同DOM/锚点、明暗×双布局×四宽、全部主题代码文字4.5:1及完整原文回退，9项 |
| `verify:a11y` | landmarks、图标名称、示例草稿不假发送、Tab、命令面板与Escape |
| `verify:fixture` / `verify:theme` | Rust投影、工具展示、指标、主题/密度/代码配色（runner提供隔离预览） |
| `verify:mobile` | 320/390视口与抽屉 |
| `verify:settings` | Provider目录/CAS写回执/一次性授权、居中原生设置窗口、分类搜索/双向Tab/焦点归还、聊天/真实iframe保持、偏好刷新、关闭清除私密输入、320/390px及断连边界，14项 |
| `verify:tree` | 目录懒加载、空态、局部失败与重试、根目录和展开分支刷新、分支移除与迟到响应、展开恢复与范围隔离、大目录可见行虚拟化、明暗桌面/320px 焦点与触控，以及草稿和真实 iframe 保留，21 项 |
| `verify:files` | Rust源码与已校验unified Diff双行号、原始patch安全回退/精确复制、零范围/多文件/极值行号、加载/失败/迟到隔离、只读GET/暂存查询、手机固定双列与局部滚动、全主题4.5:1、完整原文降级与迟到WASM，14项 |
| `verify:live` | typed读取、真实命令路径、IME、中止、队列、草稿、配置与断开不回样例 |
| `verify:hosted` | 隔离 Rust owner 同源托管、认证、项目/会话创建、改名、置顶、归档/恢复及草稿，10 项 |
| `verify:browser` | 隔离真实HTTP页面、沙箱、标签/地址栏历史/刷新、表单/选区/草稿保留 |
| `verify:shell` | 7 宽×明暗几何、独立列表、会话导航、改名及置顶回执、迟到恢复、节点能力限制及 320px 会话窗口 |
| `verify:creation` | 权威ID回读、拒绝/未知结果、只GET核对、重复激活/迟到回调/重连、草稿与弹窗 |

hosted前在仓库根目录执行：

```bash
cargo build -p work-cli --locked
cargo build -p work-node-service --bin work-server-rpc-fixture --locked
```

hosted只使用独立Work/Agent目录及现有无凭据RPC fixture；专用副本名才能启用只读空资源目录profile，默认RPC-only fixture行为不变。不启动已安装rpi、不读真实凭据、不发prompt或调用模型。它是**真实Rust owner + 合成原生协议后端**，不是实际rpi、模型、资源安装或Native窗口验收。其余live命名的脚本是隔离HTTP/WS协议fixture，不可与真实模型验证混称。CI接线不等于远程CI已执行。

截图生成到仓库忽略的 `artifacts/confirmed-workbench/`、`artifacts/reading/`、`artifacts/settings/`、`artifacts/rich-answer/`、`artifacts/files/`，不提交；日志/诊断留在 `/tmp` 或 `target/`。4193/4181及原型既有截图不由这些脚本清理。

## 已接通与尚未完成

- 外壳：46px整窗顶栏、52px快捷栏、275px侧栏，700px导航抽屉/900px浏览器覆盖层；项目置顶/折叠仅本设备偏好，会话pin/archive来自节点。
- 桌面顶栏的「上一个会话」「下一个会话」按本次连接内的访问顺序切换会话或空项目。切换复用现有读取、草稿和阅读恢复机制；从历史位置另选目的地会替换前进分支。导航根据当前项目和会话列表跳过已归档或已移除的条目，重连或刷新页面会重新开始记录。
- 「会话操作」可改名、置顶和归档当前会话，侧栏「已归档会话」提供恢复入口。节点明确公布 `session_organization` 后可改名，公布 `session_metadata` 后可置顶、归档和恢复。归档还要求当前会话已确认空闲。会话 ID 和提交值匹配的 Rust 回执更新列表。改名和置顶保持当前会话连接，恢复沿既有会话读取恢复草稿。未知结果会锁定本页对应节点和会话的管理操作。
- 创建：只登记目标节点的已有目录，`initGit=false`；按返回ID回读后切换。关闭/重连不表示取消；未知写入结果锁住，仅GET核对，不能按名称或差集猜成功。系统目录选择和目录创建禁用。
- 草稿：按节点/会话或空项目隔离，同一render取正确scope；存储被拒仍保留内存草稿。创建/拒绝/切换不清掉旧草稿。
- 浏览器：真实HTTP(S) iframe、沙箱/no-referrer，无节点凭据注入；主题/切栏/视口不重载。Web不能回读跨源页内导航或可靠判定CSP/X-Frame-Options失败，load不等于成功，保留新窗口恢复。不冒充原生完整浏览器。
- 会话（第五批已接通并本地验收）：Rust `SessionClient` 复用唯一canonical `SessionConnection`，共享 `SessionControls` 按精确会话能力、已报告字段、在线状态与canonical phase准入；TypeScript只绑定/展示。prompt/abort ACK不等于结算，dialog ACK不闭卡，配置必须取得精确权威回读；回读失败保持未知并锁定，不继续显示写入前模型为当前事实。SDK已观察到活动后恢复出的终态可结束本地受理窗口，旧终态或活动快照不能误解锁。节点/会话作用域与原生观察epoch拦截旧异步结果，项目/文件/review/install另有请求令牌。
- 回执恢复：同一绑定内保存SDK实际返回的typed `OperationRef`，只经现有 `operation_status` GET核对，绝不重新reserve/execute。共享纯Rust规则保留原HostResponse，传输Succeeded不覆盖`success=false`；受理结果不当作执行结算，草稿不会自动再发送。配置另存真实setter ACK证据，再以新权威回读确认效果；没有ref且没有ACK证据的未知结果仍保持锁定，不按runtime/list猜成功。一次核对总预算15秒，迟到旧范围GET不能解锁新范围，核对后重新读取metadata，不覆盖SDK执行投影。
- 消息阅读（第六批已接通并本地验收）：原型阅读/经典布局共用同一DOM及阅读列，16px正文、舒适/紧凑行距，偏好只影响展示。共享纯Rust `conversation::transcript_view` 从既有canonical历史、行顺序、投影和本地请求账本生成展示DTO；只将相邻工具/思考归为展示片段，不冒充运行归属。正文在过程容器之外；已知完成结算才默认折叠成功片段，失败/取消/未知工具不显示成功且保持提示。React仅记录用户展开偏好，已展开子工具不因结算收起；历史吸收流式正文保持身份，历史工具按真实结果恢复，私有/不透明内容不成为正文。未知能力的只读历史也走同一Rust展示函数，不编造phase或开放写入口。
- 第六批验证：产品/fixture双构建、15套回归、共享Rust151项、三crate WASM严格Clippy、fmt/diff及CI YAML语法通过；reading专项8项，新增Rust展示规则11项；lint70警告/0错误。既有工具输出断言保留，先展开新的完成过程再验原内容；只读降级接线回归有先失败后修复证据。节点及资源作用域另有6项独立HTTP gate时序证据。均为本地隔离协议fixture验收，不是实际Provider/安装器/模型/Native验收，未执行远程CI。
- 滚动状态（第七批已接通并本地验收）：`useConversationScroll` 只管理展示，以节点origin/项目ID/会话ID的无歧义元组隔离内存位置，沿用Rust canonical部分key和视口偏移。贴底才跟随；上翻保持锚点并可显式“回到底部”，有新内容（含等长更新）才提示未读。按钮在滚动内容外的稳定可视容器，跳转即时完成、无强制动画。空的过渡history不覆盖原锚点；MutationObserver动态观察实际内容，ResizeObserver观察内容与视口，程序补偿不重新采样用户锚点，旧scope回调不能改新scope。第七批中设置/资源页暂停聊天滚动采样，返回再恢复；第八批的设置窗口不再替换聊天，资源页的暂停规则保留。滚动/展开/选区缓存最多128个scope，展开选择每scope最多512项；均不写入协议或执行投影。父过程与子工具展开意图在SPA内切换会话/页面后恢复，草稿选区按既有draft scope保存，恢复不抢焦点、不改发送/ACK账本。
- 第七批验证：产品/fixture双构建及15套浏览器回归通过；reading由8项扩为12项，覆盖长段落中段上翻、真实text_delta→最终历史的DOM身份和锚点、显式回底/后续跟随、等长改写、两个会话双向恢复、390px及reading/classic重排、过程/工具展开、设置/资源页往返、同会话/消息ID跨两个独立节点隔离；真实HTTP iframe表单/加载次数、草稿和选区保持。空history归零、补偿后错误采样、跨scope假未读和页面卸载丢选区均有先失败后修复记录。本批没有Rust语义或协议改动；Rust151项、三crate WASM严格Clippy、fmt/diff及CI YAML复核通过；lint仍70警告/0错误。仅本地验收，不冒称远程CI或真实模型/Native验收。
- 设置窗口（第八批已接通并本地验收）：按确认原型 `settings.css` 迁入居中原生dialog，桌面宽min(92vw,1200px)/高min(85dvh,784px)、分类栏与搜索，700px以下全屏并可折叠浏览分类。只列出现已接通的外观、提供商、工作协同和关于；不搬入演示通知/环境/发现/共享开关或安装器。外观共用现有Rust主题/密度及reading/classic偏好。原生模态使背景inert，补双向Tab边界；Escape/关闭后返回真实可见入口，popover入口隐藏时返回显示选项按钮。窗口打开不卸载聊天/composer/真实iframe，不改阅读锚点或草稿选区；移动分类收起后将焦点移到内容，避免焦点留在隐藏导航。
- 节点设置仍走既有绑定与scope guards：Provider CAS revision与管理回执、Rust密封的一次性授权、节点版本均未换协议；未取得目录不冒充空目录，保存回执不声称当前Agent已加载。分类切换保留当前窗口的未提交密钥，但关闭或连接/会话作用域改变即卸载私密表单，不写本地存储；关闭不等于取消已提交写入，不会自动重发。移动端密钥表单换行且无横溢出。
- 第八批验证：原settings专项7→14项，保留所有目录/写回执/授权断言，增加实际居中几何、初始焦点/双向Tab/背景快捷键与遮罩、分类搜索、真实iframe/聊天同DOM及草稿选区、偏好刷新、关闭清密钥、320/390px和未连接边界。目录刷新从sleep改为等待精确GET响应。原6项HTTP gate scope断言未放宽，仅按新模态交互先关闭/重连/再打开并选择对应分类；reading12项保留。双构建和15套完整回归、Rust151项、三crate WASM严格Clippy、fmt/diff及CI YAML复核通过，lint69警告/0错误。截图在忽略的artifacts/settings；没有远程CI或真实模型/平台验收声明。
- 富内容与复制（第九批已接通并本地验收）：共享Rust `conversation::markdown` 用pulldown-cmark输出白名单树与可读复制载荷，WASM只序列化DTO；React不解析Markdown、不生成执行/消息身份。支持标题、段落、强调、列表/只读清单、引用、表格、围栏/缩进代码；代码使用既有Rust有界lexer，未知语言保留文本，未闭合围栏支持流式展示。Rust现有code背景token接入CSS；小字号代码色由共享Rust按4.5:1保留候选或仅该角色回退正文色，不改全局accent。Reddit浅色关键字4.476:1问题先由全主题浏览器审计复现，再修复。
- 安全/边界：原始HTML只作转义文本，Markdown图片只显示说明、不请求src；仅显式HTTP(S)、无凭据/控制字符/反斜线的链接可由用户在新窗口打开（noopener/noreferrer/no-referrer）。相对/文件/应用scheme不映射成节点/平台操作；无数学/图表执行器、自动嵌入或远端资源加载。超过1MiB、32768解析事件或48层栈边界时显示完整原文并提示，不截断canonical内容。不是完整Markdown扩展或文件跳转实现。
- 复制回答是Rust生成的可读纯文本，不是原始Markdown：段落空行、列表换行、表格制表符，不含作者/按钮/工具。代码复制是解析后的代码字符串（CommonMark会规范化换行）。只走显式手势Clipboard API，不走execCommand或改变草稿选区的后备路径；失败清成功标记并提示手选，pending禁重入，载荷变化/卸载使迟到反馈失效，A→B→A不复活旧成功。不能撤销已经发出的系统剪贴板写入；本批不替换工具/文件registry组件自身的复制实现。
- 第九批验证：新增rich9项进入同一runner/CI，产品/fixture双构建和全部16套通过；Rust151→161（Markdown9项、代码色1项），WASM绑定6项，三crate WASM严格Clippy、fmt/diff和CI YAML通过，lint仍69警告/0错误。覆盖真实协议fixture历史与流式转历史、代码区/正文/anchor同DOM、上方富内容增高后<2px阅读锚点、草稿/选区及真实HTTP iframe保持；精确载荷和成功/拒绝/缺失/同步异常/迟到仅用隔离剪贴板桩，未写宿主剪贴板。原settings14/scopes6/reading12等断言均保留。截图实际复核也发现并修复段落margin被清零；截图仅忽略artifacts/rich-answer。
- 键盘与平台证据：默认验收覆盖Tab进入、Tab离开/Shift-Tab返回、focus-visible内侧outline计算值和截图、局部scrollLeft及整体无横溢出；1440/1024/390/320×明暗×双布局和所有可选主题代码文字对比度通过。Chromium在连续窗口重排后原生ArrowRight横滚路由偶发不动作，和原型既有代码焦点脚本一样保留为 `RPI_WORKBENCH_NATIVE_SCROLL_PROBE=1` 的可选平台诊断，不当成默认稳定门槛、不以自造快捷键替代原生行为，也不宣称各平台键盘手验完成。
- 只读文件工作面（第十批已接通并本地验收）：源码与原始Diff不再经过registry的TS高亮器；新增共享Rust `file_view`复用既有有界 `code_tokens::tokenize_file`。源码真实分行/行号，Diff只按原始前缀着色、不生成hunk旧/新行号或反馈目标；无HTML执行、编辑保存、行反馈和暂存写操作。触及1万换行或超过1MiB时退为单个完整原文节点，停用行号/高亮并提示，不截断正文或误报为1行。源码CRLF显示分行规范化，复制仍是节点返回的原字符串；Diff也复制完整原始patch，不带行号/按钮。
- 文件请求继续由既有NodeConnection读取与fileTicket/projectScope保护，TypeScript只保存判别联合的本地读取展示状态。开始新请求立即显示准确路径/模式/范围和加载态，清除旧内容；失败在面板内提示，可明确重试同一GET，不发写请求、不自动重试。空文件不冒充加载，空Diff不误判成源码；暂存勾选只是查询范围，源码时不触发Diff，改动行/树项都会携带真实筛选，文件面板重挂从当前Diff恢复筛选不自动再读。仅显示引擎就绪才挂载目录树/文件视图，迟到WASM不调用未初始化导出或缓存空目录。
- 文件布局沿原型的标题/只读状态栏、独立内容滚动、行号列和内侧焦点；短行与长行共用宽度轨道，使横滚超过一屏后所有行号仍在视口内。改动文件入口是可键盘激活的按钮，手机44px触控，HTML源码只显示文本。复制复用第九批的错误/迟到安全控件并独立其CSS，不改变聊天/草稿/选区或真实iframe；全部5主题明暗源码token/行号及Diff前缀/标题对比度>=4.5。原生方向键平台限制仍沿用上条边界，不声明全面平台键盘验收。
- 第十批验证：新增files11项进入runner/CI，产品+fixture双构建与17套完整回归通过；共享Rust165项（新增文件展示4项）、WASM绑定7项、严格三crate WASM Clippy、fmt/diff/YAML通过，lint67警告/0错误（移除旧面板死代码减少2项，未压制规则）。旧产品缺文件阅读区、长横滚短行行号消失、迟到WASM无加载保护均有先失败后修复证据。完整回归另复现hosted原断言抢在SDK请求前检查侧栏选择，保留精确ID断言并增强为等待该ID的成功snapshot响应完成，不改owner或加sleep。截图实际复核，仅忽略artifacts/files；没有远程CI、真实模型或Native验收声明。
- 目录读取（第十一批已接通并本地验收）：保留既有NodeConnection/listDir的错误传播及Rust endpoint/作用域保护，只补树组件的本地展示状态。根/展开的子目录明确显示加载、成功空态或路径对应的失败；失败不冒充空目录，也不显示为聊天发送错误。仅明确重试才使headless-tree该目录的children缓存失效并重读同一GET，pending无可重入按钮；折叠/重新展开使用已观察的成功或失败缓存，不自动重发。其他目录反馈及已选源码不被清除；卸载/跨scope迟到响应不进入新树。没有新增协议、依赖、写动作或执行owner。
- 第十一批验证：原tree专项移除固定sleep并扩为10项，已连接但未选项目时不挂树、不请求文件、不冒充空根；根目录HTTP gate、局部503/精确GET重试、空根/空子目录、独立子目录反馈/既有源码DOM、折叠缓存、跨会话旧成功/旧失败对新loading的隔离、原生ArrowRight/Enter/Space、实际Tab往返及内侧焦点、44px、明暗1440/320、草稿选区和真实HTTP iframe同DOM/表单/加载次数均通过。产品/fixture双构建、17套完整回归、共享Rust165/WASM绑定7、三crate WASM严格Clippy和格式/结构检查通过，lint仍67警告/0错误，未压规则。截图仅忽略artifacts/tree，日志/tmp；本批不声明通用目录刷新、分页/虚拟化、刷新后树状态持久化、远程CI或平台/真实模型验收。
- 结构化Diff（第十二批已接通并本地验收）：新增共享Rust `diff_view`，复用有界原始行展示，先完整检查hunk语法、计数、顺序及checked行号，再一次性发布旧/新行号。支持省略count、零范围、多hunk/文件、CRLF、Unicode和no-newline；hunk中的`---`/`+++`是正文，不冒充文件头。保留全部metadata/原始符号，源码与Diff复制仍是完整节点原字符串。范围损坏/截断/溢出、未知行及binary/combined等不支持格式，整份回退原始Diff并说明，不保留局部推算数字；超大输入沿既有完整原文降级。此检查只用于展示，不验证路径、真实仓库版本或patch可应用性，不生成review身份/权限，也没有行反馈、编辑或Git写入。
- 双列沿确认原型sticky，Rust提供所需数字位数、CSS统一列宽（至少40px，u32极值不裁字），长横滚后两列均固定；hunk/note只读标记，屏幕阅读器可读旧/新行标签，内侧焦点和手机44px不变，复用全部现有主题token。扩files11→14项（runner仍17套）：先在旧build复现hunk正文误判；保留raw回退的无行号/精确copy断言，加真实双行号/CRLF/HTML inert、零范围与跨文件重置、损坏/combined/binary/metadata回退、极值列宽，并把明暗四宽横滚及全5主题审计扩至两列/hunk/note；草稿/真实iframe与scope断言不放宽。产品/fixture双构建、17套回归、Rust172/WASM8、三crate WASM严格Clippy/fmt/diff/YAML通过，lint67警告/0错误。无新依赖、协议或旧UI修改；截图实际看并仅忽略artifacts/files，日志/tmp/rpi-diff-*，不声明远程CI/真实模型或平台验收。
- 目录刷新（第十三批）：目录页的刷新先读取项目根目录，再按父目录优先的顺序读取点击时已展开且仍可见的分支。每个目录使用既有只读 GET。等待或失败时保留上一次成功列表，局部重试只读对应路径。成功响应更新目录项，保留仍存在的目录的展开状态，清理已移除或变更类型条目的缓存与树状态。跨会话或已移除目录的迟到响应不进入当前列表。
- 页面刷新后的阅读位置（第十四批）。同一浏览器按节点、项目和会话保存滚动像素、是否跟随底部及可用的历史消息锚点。当前会话历史就绪后按 Rust 提供的消息标识恢复原位置。锚点缺失时，将保存的像素位置限制在当前内容的可滚动范围内。刷新沿既有会话读取流程，存储不可用时仍可在页面内切换会话并恢复位置。
- 目录展开恢复（第十五批）。浏览器按节点、项目和会话记录展开过且未折叠的目录。刷新页面后，打开「文件 → 目录」会从根目录开始恢复可见分支。折叠父目录会延后其后代读取。目录读取失败可单独重试，成功响应确认路径消失或变为文件后会清理该分支。浏览器存储不可用时，本页仍会记住展开状态。
- 大目录（第十六批）。展开后的树超过 80 行时，只挂载视口附近的行及焦点行。目录条目顺序、键盘跳转和无障碍层级仍由完整的已加载树提供，文件按节点返回的路径读取。长文件名在虚拟行内单行省略，完整名称保留在行标签和指针提示中。现有 GET 仍返回整份目录列表，服务端分页需要节点协议提供分页契约。
- 工具展开和草稿选区在同一页面内恢复。真实模型长会话、各平台手动验收、其余消息细节、设置能力、真实协同、本机桥接、Native/Android 适配和正式发行仍待完成。Provider 管理回执确认节点保存结果。运行解锁仍依赖可关联的新结算，缺少身份引用或原 setter ACK 的未知结果会保持锁定。

组件仅使用免费MIT的ReUI/shadcn生态；许可证说明见 [NOTICE.md](NOTICE.md)，没有引入Pro资产或授权密钥。

## 同源托管产品产物

在仓库根目录执行；先通过安全环境配置非空 `RPI_WORK_HTTP_PASSWORD`，且不能与另一个owner共用同一数据目录同时启动：

```bash
RPI_WORK_HTTP_BIND=127.0.0.1 RPI_WORK_HTTP_PORT=13031 RPI_WORK_WEB_LISTEN_PORT=13032 \
RPI_WORK_HTTP_ASSETS="$PWD/apps/workbench-ui/dist" \
  cargo run -p work-cli --locked -- serve
```

打开 `http://127.0.0.1:13032/` 走同源认证。仅托管产品 `dist/`，不托管 `dist-fixture/` 或测试源码；本说明不是安装发布或Native/Android完成声明。
