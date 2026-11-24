# 已确认原型 → 真实 UI 迁移

## 基准与边界

用户已确认 `design/reui-message-layout/` 当前完整工作台原型（含右侧浏览器、节点项目列表和工作协同），要求 1:1 实现真实 UI。该确认取代先前“等待设计评审”的阶段状态，不代表演示状态机成为生产协议。

当前唯一实现/构建入口是已接入 Rust WASM 的 React/ReUI 前端 `apps/workbench-ui/`，`design/reui-codex/` 只保留迁移说明。保留旧 `apps/work-ui`、主 checkout、4193/4181 服务及既有未跟踪截图；仅在 `feat/next-iteration` 本地提交，不推送。整套真实 UI 尚未完成，不能以局部功能或 fixture 截图宣称交付。

新 UI 源码和产品构建已移出 `design/`，产品模块图不导入测试样例；正式打包/发行仍未完成。Desktop/Web/Android 应使用同一 presentation，差异限于平台能力适配。当前 React 工程的 Web 运行不等于桌面本机桥接和 Android 已完成，不能让桌面本机经回环 HTTP 绕过既有 owner 门面。

## 已有真实链路

- `src/lib/node.ts` → `work-client-wasm` → `WorkspaceClient` / typed NodeApi；保留协议、认证与 endpoint 的 Rust 所有权。
- `src/lib/workspace.ts` 接项目、会话、历史、实时投影、发送、中止、队列、配置、源码、资源及供应商；需逐项复核能力观察、scope/generation/epoch、ACK 与 settlement，不直接把旧代码存在当成完成证明。
- `src/lib/viewmodel.ts` → `work-viewmodel-wasm` / RunProjection；不在 TypeScript 另造消息行序、工具摘要或执行状态机。
- `src/lib/theme.ts` 消费 Rust 主题 contract；展示偏好不得改变 Agent 配置和权限。

## 逐项迁移与验收表

| 范围 | 当前差距 / 真实入口 | 验收要求 |
| --- | --- | --- |
| 窗口外壳、导航、项目/置顶/最近 | 第二批已迁移整窗外壳和独立列表；只展示当前节点入口身份，多节点聚合/平铺设置仍待接入 | 1440×940 DPR1 明暗同内容对照；无伪造节点/连接状态 |
| 阅读列与消息层次 | 第六/七/九批已迁入阅读/经典、完成折叠、阅读锚点及共享Rust富内容/复制；扩展消息细节仍待核对 | 流式、失败、历史恢复、滚动锚点、草稿/选区保留 |
| Composer、模型、思考、队列与审批 | 第五批已接通并本地验收：canonical绑定、共享Rust准入、ACK/结算分离、真实选项单次回复及typed回执只读恢复；整体UI迁移仍未完成 | IME、accepted 未确认、迟到回调、能力变化、附件边界 |
| 右侧浏览器 | 首批施工：真实 HTTP(S) 页面、多标签、地址与历史、刷新、视口 | 隔离的真实 HTTP 页面；拒绝危险协议/凭据；不传节点密钥；隐藏不重载 |
| 源码、目录与 diff | 已有 NodeConnection 文件/Git 调用；布局与原型不同 | ticket/revision、长行固定控件、主题对比度、仅能力允许的写入 |
| 居中设置与本地偏好 | 第八批已接通真实居中设置窗口及既有外观偏好；未支持能力不搬演示状态 | 搜索、焦点恢复、原型布局；本地偏好与执行配置分离 |
| 工作协同与多节点 | 原型里的发现/开关/授权为 fixture，不得照搬为真实成功 | 真实节点授权、独立入口、过期/撤销与权限隔离；不支持时明确禁用 |
| 项目添加与本机 rpi 设置 | typed 项目登记/会话创建已接通；目录/程序选择、安装仍需平台桥，原型下载是假流程 | 能力明确、真实校验；浏览器不能假装发现绝对路径或执行安装 |
| 终端、Git/Worktree、调度与扩展 | 逐项核对既有 typed API；不猜 endpoint | 未支持则说明/禁用；不添加模拟后端或第二个 owner/调度器 |
| 入口与部署 | 第四批已迁至apps/workbench-ui，产品无样例依赖，fixture仅独立构建+显式参数启用；正式打包与平台桥待完成 | 认证/加载/错误/空态、同源托管、完整发行与Native/Android能力验收 |
| 可访问性、响应式与验收 | 保留旧回归并新增原型对照门槛 | 320/390/700/1024/1440、明暗、键盘、触控、真实测试与 fixture 结果分别记录 |

## 第一批：真实浏览器（565d3ac）

1. 已识别真实 React/ReUI 与 Rust WASM 接口边界，TypeScript / WASM / Vite 构建通过；当前 lint 为75个警告、0错误，多数来自既有 registry 组件，未将警告当作已消除。
2. 首批浏览器已接入真实 HTTP(S) 页面：多标签、地址校验、地址栏历史、刷新、视口和新窗口打开。切换主题/标签/面板及改变视口不重建 iframe，保留未保存表单、输入草稿和选区；不复制原型的假 README 页面或 `rpi://project/` 作为真实站点。
3. Web iframe 使用不含 same-origin 权限的沙箱和 no-referrer，不注入节点认证或父页面桥；浏览器自身 Cookie/网络策略仍由浏览器负责。跨源页内导航无法回读，历史仅记录地址栏发起的导航；无法可靠判断站点嵌入策略，load 不代表内容成功，始终提供新窗口恢复路径。原生完整浏览器能力仍须平台适配，未假装完成。
4. 全部9组测试通过（a11y / fixture / theme / mobile / settings / tree / live / hosted / browser）。其中 live 是隔离协议 fixture，hosted 起真实 Rust 节点并托管本次 dist，browser 使用隔离真实 HTTP 页面；没有调用模型服务或对外站点。结构检查与 diff 检查通过。回归同时修复命令面板可见后输入框尚未获焦时 Escape 不关闭的时序问题，并补失焦断言。
5. 1440/1024/390/320 DPR1 明暗截图由专项脚本生成到 `artifacts/confirmed-workbench/`，不提交；已复核桌面浅色和手机深色。它们是局部真实接线证据，不是已确认原型的整窗像素验收。Chromium 既有换主题/重排后嵌入页偶发不重绘仍独立记录，不以自动重载丢弃页面状态；展示截图从新上下文生成。
6. 该批尚未迁移外壳、消息、设置、真实协同及正式发布；不是整套功能完成声明。4193 缺失的原型服务已在原目录恢复，4181 保持运行，原有未跟踪截图未改动。

## 第二批：整窗外壳与独立列表

- 迁入46px跨整窗顶栏、52px快捷栏和275px侧栏；700px导航断点、900px预览覆盖层断点。1440×940 DPR1实测原型：顶栏 `(0,0,1440,46)`、快捷栏 `(0,46,52,894)`、侧栏 `(52,46,275,894)`、阅读区 `(327,46,566,894)`、浏览器 `(893,46,547,894)`；新外壳以相同几何回归，预览宽度按整像素取整，旧浏览器保留DOM而非重载。
- 项目与会话列表解耦，最近是扁平列表；折叠/项目置顶仅为按节点入口隔离的本设备偏好，不改变当前会话、最近顺序或服务端权限。会话置顶/归档直接采用 Rust 客户端返回的字段，不伪造本机/远程节点归属，不在 UI 写会话元数据。真正的空节点显示零项目/零会话，不生成名为“空节点”的项目。
- 原“新对话”只清草稿的假行为已移除；创建会话/项目、状态筛选、归档管理、会话目标、计划任务及会话导航历史尚未接入时明确禁用。保留已有 typed 连接、消息、文件、资源和供应商调用；显示选项使用原生弹层，快捷栏的真实设置/资源入口仍可调用旧页面。
- 新增 `verify:shell`：1440/1024/900/701/700/390/320 × 明暗14档几何、偏好持久化/节点隔离、独立列表、节点 pin/archive 字段、真实空态、未支持入口、草稿保留；已有浏览器测试新增侧栏开关/项目置顶不重载iframe或丢选区。截图仍只生成到 `artifacts/confirmed-workbench/`，有限动画在截图时禁用；已单独证实主题按钮的瞬时暗色截图是150ms过渡，不以延时掩盖协议结果或自动刷新网页。
- 全部10组测试、WASM/TypeScript/Vite构建、结构/diff检查通过；lint当前73警告、0错误，未宣称清除既有警告。测试调用只针对隔离fixture/临时真实Rust托管节点；没有模型调用或对外连接。
- **该批仍未完成**：正式入口/构建迁出design、默认fixture显式隔离、真实创建与权限门槛、阅读/经典消息、居中设置、真实工作协同和原生能力适配。当前截图仅证明外壳几何与局部接线，不是整套1:1像素验收；三张原有未跟踪截图继续保留，4193/4181不变。

## 第三批：真实创建、权威回读与作用域保护

- WASM 仅新增既有 `WorkspaceClient::create_project` / `create_session` 的薄封装，TypeScript不写endpoint、认证协议或第二个owner。项目只登记目标节点上已存在的目录，`initGit=false`；浏览器不扫描本机、不上传目录、不创建目录，系统目录选择明确禁用。
- 新建操作先取得节点返回ID，再从真实项目/会话列表按该ID核对，不乐观插入本地假数据。同步引用锁防重复点击；Rust区分明确拒绝与未知写入结果。网络/5xx/异常回包不能推断未提交，结果未知时保持锁；“核对创建结果”只读列表，不重发写入，也不凭名称/列表差集推断成功。关闭弹窗和重连都不表示取消节点请求。
- 连接建立、登录、会话历史、创建列表读回、运行观察、队列与实时回调在该加载链上核对连接epoch/会话加载ticket；创建回调另核对原选择代次。迟到成功只刷新同一连接的节点事实，不抢回用户刚选择的会话；旧连接的错误不能污染同地址新连接。Rust presenter缓存键纳入节点入口与epoch，避免同名ID跨连接共用投影。
- 添加项目弹窗沿用确认原型，原生dialog实测宽度为1440下450px、390下352px、320下282px（包含Chromium的原生max-width约束）；支持Esc关闭/焦点归还，拒绝后保留表单内容，手机输入区44px。空项目禁发/禁入队，提示先创建真实会话；这不等于完整的 `SessionAccess` 权限/settlement 门槛已验收。
- 空项目草稿按节点/项目隔离，会话草稿按节点/会话隔离；草稿值随scope同步派生，不等effect再显示旧scope文字。修复存储被拒时切会话丢失内存草稿的问题。创建、拒绝、切换和返回不会用“清草稿”冒充新建。
- 新增 `verify:creation` 10组：成功与拒绝、同回合重复激活、未知结果锁/只GET核对、已提交但读回失败、迟到成功/拒绝、同地址重连、迟到历史、存储拒绝、空项目及三宽×明暗弹窗。完整11组前端套件、WASM/TypeScript/Vite构建通过；lint72警告、0错误。Rust binding 2个单测及原生 `session_services` 合同测试通过，WASM binding及原生fixture严格Clippy通过；格式检查针对两个修改的Rust文件，未宣称全workspace测试已跑。
- **收紧hosted验收隔离**：旧脚本虽隔离Work数据，但未固定rpi程序/Agent目录，先前结果不能证明原生环境隔离。本批强制独立Work/Agent目录及现有无凭据RPC fixture；实测RPC-only fixture因缺少安全资源CLI返回501，没有退回已安装Agent。仅对专用fixture副本名增加只读空目录profile（`loadedFacts=false`，不支持安装、构建、网络或exact selection），默认fixture行为不变。当前真实Rust owner + 合成原生协议后端的8项托管/创建/草稿验收通过；这不是实际rpi、模型或资源安装验收。
- CI补入 settings/tree/browser/shell/creation；未推送或声称远程CI已运行。截图只在 `artifacts/confirmed-workbench/`，该批结束时正式发布、默认fixture隔离、完整权限/结算链、消息/设置视觉、真实协同及Native/Android能力仍待后续批次。

## 第四批：正式源码入口与编译隔离的样例

- 唯一React实现迁至 `apps/workbench-ui/`，旧design目录仅保留迁移说明；没有复制第二套UI，也未改旧 `apps/work-ui`。产品build/preview与同源托管只使用新目录的 `dist/`，`dist-fixture/` 为独立测试构建，不覆盖或分发给产品。旧根Task的Dioxus启动路径未擅自替换，目录迁出不是完整安装/发布声明。
- Vite按构建mode选择初始快照adapter：产品模块图只导入空快照，测试构建才导入 `tests/fixtures/workspace.ts`，且还须 `?fixture=1`。生产URL加此参数也不能启用样例。默认、静态托管和断开都是零项目/零会话/未知指标；未连接不显示虚构节点组。开发代理移除隐含13031，必须显式配置 `RPI_NODE_URL`；真正托管只探测自己origin的公开入口元数据，认证及协议仍走Rust客户端。
- 样例明确标注“示例 · 未连接”，发送/创建不假装可用；只在显式重新打开样例入口时恢复它自己的显示偏好。运行观察缺失不再回落样例模型/思考/成本/上下文，真实模式不填入样例步骤或工具耗时；认证拒绝显示真实错误并保留草稿。
- 新增 `verify:entry` 8项：产品默认与伪fixture参数不出样例、产品JS无样例载荷、测试构建需显式启用、认证失败无可写回退、节点模型已观察但指标/上下文缺失保持未知、显示WASM迟到仍呈现权威历史、迟到合成ACK保护后来编辑、同地址重连区分未连接与节点会话草稿。先跑旧逻辑实证默认入口失败，再修复；另以HEAD旧草稿hook复现发送A后编辑B再改回A，被迟到ACK清空，新hook以每次本地编辑对象作令牌，阻止此丢稿，不在TS新造执行状态机。延迟真实显示WASM也先复现历史不可见，再让现有loadSession等待显示初始化并把展示就绪加入行投影依赖，避免提前订阅丢事件。
- `verify:all` 现在自起free-port测试预览，按序运行12组，记录并仅清理自己启动的进程组；自包含脚本直接启动Vite进程，避免只杀npm父进程。全部12组本地通过，产品/fixture的Rust WASM+TypeScript+Vite构建通过，lint71警告/0错误；hosted仍为隔离真实Rust owner + 无凭据合成RPC后端8项，entry的prompt ACK来自HTTP/WS协议fixture，不是模型或真实Agent执行证明。
- CI改用 `workbench-ui.yml`，按新目录分别构建产品和fixture，新增入口隔离验收；未运行远程CI、未推送。确认原型、4193/4181与原有三张未跟踪截图保留。**仍待完成**：完整SessionAccess/权限/accepted与settlement链，阅读/经典消息及滚动、居中设置、真实协同、Native本机/Android桥接、依赖许可分发核对和最终完整UI验收。

## 第五批：会话准入、结算窗口与异步作用域（已接通并本地验收）

- `work-viewmodel::session_controls` 集中纯Rust准入；`work-client-wasm::SessionClient` 只保留请求账本及展示DTO，复用现有 `WorkspaceClient.connect_session_with_access` / `SessionConnection` 的snapshot、cursor、重连、消息身份、reconcile和RunProjection，不另建执行owner。精确access读取失败撤销旧写入口，不借global支持猜权限；可选configuration失败不抹掉此次真实读到的prompt能力。目录及运行字段未观察到时不允许配置。
- prompt/abort受理不等于运行结算，权限回复ACK不关闭canonical卡片；只提交当前观测到的真实字符串选项，账本阻止闭卡前重复回答。模型以原始provider+id区分，模型/思考写后必须精确权威回读；已受理写入后的回读拒绝属于结果/效果未核实，保持锁定并撤销过时运行字段，不能当作写入被拒绝。已观察到活动后，SDK重连恢复的终态可结束已知本地窗口；Ready、未开始请求对应的旧终态、仍活动的快照不能误当本次结算。
- 连接/会话ticket、binding身份及原生观察epoch保护异步结果；项目scope、review/file/listing请求票据与install对象令牌阻止A→B→A、取消后复活及旧finally解锁新操作。节点Provider/版本/二维码使用节点scope；管理回执只说明保存及节点是否要求重启，不声称当前Agent已加载。
- typed只读恢复已接通：同一绑定的Rust请求账本只存SDK实际返回的OperationRef，调用现有 `operation_status` GET；不生成ref、不拿本地scopeEpoch当ownerEpoch、不reserve/execute重发。`work-viewmodel::session_receipts` 保留SDK KnownResult/Failed/OutcomeUnknown语义及原HostResponse，传输Succeeded内的success=false仍是拒绝。配置额外存实际setter返回Ok的ACK证据，只有这种已知ACK才允许仅用新权威回读核实效果；ACK前无ref的未知结果保持锁定。核对总预算15秒，迟到旧范围GET不消费新范围账本；核对结束重启metadata只读观察，防止旧观察因serial失效后永久缺权限。恢复受理不代表结算，不自动清理/发送草稿或乐观关闭dialog。
- 当前本地产品/fixture双构建和14套回归通过；session专项23项，覆盖丢prompt/abort/模型/queue ACK、GET-only且reserve/execute次数不变、HostResponse拒绝、receipt仍未知不借runtime猜成功、无ref无ACK证据锁定、跨重连迟到GET和metadata恢复。最终复审按真实SDK形状补齐queue回执：解开原HostResponse并要求其data.accepted为真boolean，错误command不采纳。重连终态、回读拒绝误解锁、恢复核对打断metadata、queue回执封装均先实证失败再修复。共享Rust单测140项通过，三crate WASM严格Clippy、fmt/diff检查通过，lint仍71警告/0错误；未推送、未运行远程CI。协议fixture回执改为进程级store与唯一ID，HTTP只读状态形状沿用现有owner协议，不新增执行协议。
- 新增scope专项6项，均以独立HTTP gate固定时序：同地址重连后旧Provider目录/写回执不得污染新目录、提示及未保存密钥；迟到QR成功/错误不替换新授权；会话A→B→A并取消新审阅后旧bundle不能复活；同项目不同会话的旧install finally不能清空新请求令牌或使其永久busy。每个gate有deadline及finally释放，使用真实SDK脱敏错误分类而非猜原始服务错误会透传。这些是隔离协议fixture证据，不代表真实Provider或安装器验收。
- **不等于完整UI交付**：没有可关联的新结算不能借Ready或旧终态解锁；没有ref且没有原setter ACK证据不能按runtime/list推断自己的操作成功。阅读/经典、滚动、居中设置、协同、平台桥及正式发行仍属总目标剩余范围。

## 第六批：消息阅读与过程层次（已接通并本地验收）

- 迁入确认原型的阅读/经典样式：阅读为靠右气泡，经典输入与正文共享阅读列；16px正文、1.65舒适/1.55紧凑行距。切换仅改展示属性，不重建消息/工具/iframe，不影响Rust执行准入；偏好刷新保留，无效值回退，存储拒绝保留内存选择。
- 共享纯Rust `conversation::transcript_view` 只读既有canonical历史、TranscriptOrder顺序、SDK投影及本地请求账本，输出正文、连续工具/思考展示片段和明确的观察文案/色调。展示片段不当作run，不能按工具结束推断Agent结算；未知请求、等待结算、重连和旧完成状态不默认折叠新请求。只在已知完成且片段工具均成功时默认收起；失败/取消/未知工具不显示成功，失败观察独立于工具成功与否。React仅管理用户折叠偏好，用户展开子工具或过程后不被结算/布局切换覆盖，未交互的已完成片段不因下一轮开始全部重新展开。
- 正文在过程容器之外，canonical内容块身份用于流式转历史及工具去重；历史toolResult不当作回答，缺结果/缺明确bool证据不猜成功。正文复用core公开文本allowlist，redacted思考、签名、图片及未知内容不混进回答；当前runtime模型不伪装成历史回答的作者模型。没有会话能力的原有只读历史回退也走相同Rust展示函数，不创建运行投影、不开放写入口；这条新接线曾导致只读正文为空，已由原session专项实证失败并修复，另补过滤与未知phase断言。
- reading专项8项：实际测量两布局边界、字号及舒适/紧凑行距，展开工具后覆盖明暗/390px无横溢出；草稿选区、消息/工具/真实HTTP iframe DOM、表单及加载次数保持；默认完成折叠与键盘操作；工具完成不等于结算、canonical结算自动收起、流式最终记录DOM不替换；子工具/父过程用户意图保持；失败诊断；历史工具唯一恢复与私有内容过滤，包括能力404的只读降级。旧live工具输出断言全部保留，只增加先展开完成过程的交互。
- 本地产品/fixture双构建和15套回归通过；共享Rust151项（本批新增11项），三crate WASM严格Clippy、fmt/diff及CI YAML语法通过；lint70警告/0错误。截图只放忽略的artifacts；原型截图和4181/4193服务保持。仅本地提交，不推送，不冒称远程CI或真实模型/Native验收。
- **不等于完整UI交付**：会话阅读位置/滚动锚点、其他消息细节、居中设置、协同、平台桥和正式发行继续作为后续范围；原型演示步骤/重试/计划实施没有被伪造为真实接口。

## 第七批：滚动跟随与SPA阅读位置（已接通并本地验收）

- `useConversationScroll` 只消费Rust展示DTO的canonical部分key，按节点origin/项目ID/会话ID元组隔离内存位置，不重新拥有消息身份、运行状态或协议。贴底才自动跟随，上翻后用可视key+偏移保持阅读；新内容（包括等长改写）提示未读，普通上翻仅显示“回到底部”。显式回底按钮固定在滚动内容外的可视容器，支持原生按钮键盘交互，使用即时跳转而非强制动画。
- 切回会话时空过渡history不覆盖保存位置，实际canonical内容就绪才补偿；锚点不在当前页时有界回退到保存的像素位置，不猜造历史或新增分页协议。MutationObserver在异步内容出现后重新观察真实阅读列，ResizeObserver同时观察内容与容器，窗口/布局/过程展开重排沿用原锚点，程序补偿不被当作新的用户阅读位置；observer清理和scope guard防止旧回调污染新范围。设置/资源页不采样聊天位置，返回恢复。缓存最多128个scope，不做刷新或跨设备位置同步。
- 以同scope的有界展示缓存恢复父过程及子工具选择（每scope最多512项），不把默认折叠解释为执行成功。草稿选区沿既有draft scope保存，只在文字匹配时恢复、不抢焦点或干扰IME；原草稿edit token和ACK保护未改。真实iframe不重建，不丢表单、不增加页面加载次数。
- 扩展现有reading专项8→12项，不另造测试runner：长段落中段上翻、text_delta→最终历史的同DOM身份及锚点、显式回底与后续跟随、等长更新提示；两个真实fixture会话双向恢复及阅读/经典/390px重排；过程/工具展开、设置/资源页往返、草稿选区及真实HTTP iframe；两个独立节点的相同会话/消息ID不能串位置和跟随意图。会话空history归零、补偿后重采样漂移、跨scope假未读和页面卸载丢选区均先实证失败再修复；未降低2px几何精度或用任意sleep猜事件完成。
- 产品/fixture双构建、15套浏览器回归、Rust151项、三crate WASM严格Clippy、fmt/diff和CI YAML复核通过，lint仍70警告/0错误。本批不改Rust语义、SDK owner或任何执行协议。截图仅忽略的artifacts，4181/4193服务及三张原未跟踪PNG保持；仅本地提交、不推送，不宣称远程CI、实际模型长会话或Native/Android验收。
- **整体仍未完成**：刷新/重启后的阅读持久化未实现；真实模型/各平台复验、其他消息细节、居中设置、真实协同、本机桥接和正式发行仍是后续范围。此前各批的“待完成”记录是当批边界，不作为后续完成声明。

## 第八批：居中真实设置窗口（已接通并本地验收）

- 沿用确认原型 `settings.css` 的桌面居中尺寸、圆角、导航/搜索、阅读列及窄屏分类结构，新增 `SettingsDialog` 原生模态外壳。只展示当前已接通的外观、提供商、工作协同、关于；不把原型通知、远程发现、访问开关、环境/安装演示包装成真实能力。外观绑定已有主题/密度和reading/classic偏好，继续使用Rust token。节点管理仍走既有Rust客户端/DTO，不改协议或执行owner。
- 设置不再替换聊天页，打开/关闭不卸载transcript/composer/真实iframe；底层草稿、选区及浏览器表单保留。原生模态使背景inert，补齐Tab/Shift-Tab首尾环绕，禁止背景⌘K再打开命令面板；Escape和关闭按钮归还焦点，popover入口已隐藏时回到显示选项按钮。700px以下全屏、可折叠浏览分类，选分类后焦点进入内容而不是留在隐藏按钮，密钥表单不横向溢出、触控输入/按钮至少44px。
- 搜索只过滤实际分类/关键词，不提交操作。分类切换保留当前窗口尚未提交的密钥，关闭或连接/项目/会话scope改变即卸载私密表单，密钥不落本地存储；窗口关闭不等于取消已提交写入，不会自动重发。Provider目录、CAS revision、完整管理回执及“当前Agent加载状态仍未知”的原规则保留，未取得目录不冒充节点空目录；授权payload仍由Rust密封，Web更新仍明确为平台边界。
- settings专项原7项扩为14项，保留Provider/写回执/授权全部原断言，增加原生居中几何/初始焦点、分类搜索、双向Tab/遮罩/背景快捷键、Escape及可见入口归还、聊天/草稿选区/真实iframe同DOM及加载次数、偏好刷新、私密输入关闭清除、320/390px和断连边界。旧产品先实证无居中dialog；初始焦点与Tab尾部进入浏览器工具栏的问题也分别先失败后修复。刷新目录改为等待精确GET响应，不靠sleep；导航hover收窄至导航，密钥保存按钮颜色不被覆盖也有先失败后修复断言。原scopes6项HTTP gate依旧验证迟到目录/写回执/QR/资源，只有模态要求的关闭→重连→重开及分类定位调整；reading12项几何/身份断言保留。
- 当前产品/fixture双构建与15套完整浏览器回归通过；共享Rust151项、三crate WASM严格Clippy、fmt/diff及CI YAML复核通过，lint69警告/0错误；未压制静态检查规则。截图已实际查看，只放忽略的artifacts/settings；保护服务及三张原PNG不动。仅本地提交、不推送，未运行远程CI，不冒称真实Provider/安装器/模型或Native/Android验收。
- **总目标继续active**：居中设置外壳不是完整设置/协同能力；其他消息细节、尚未接通设置、真实协同/本机桥、实际模型长会话与平台手动复验、刷新阅读持久化及正式发行仍待后续实施。

## 第九批：回答富内容与精确复制（已接通并本地验收）

- 原真实回答仍是单个原样文本段落，先以旧产品构建复现“富内容验收”标题没有语义heading；没有把原型假会话输出接入真实入口。新增共享纯Rust `conversation::markdown`：pulldown-cmark 0.12.2关闭默认HTML renderer/CLI features，通过白名单DTO展示标题/段落/强调/列表与只读清单/引用/表格/代码。代码沿用 `code_tokens::tokenize`，React仅useMemo调用WASM和布局，不另写Markdown/lexer或执行owner，不改canonical消息key、SessionConnection与ACK/结算规则。
- 无任意标签/属性DTO、危险innerHTML或图片src。原HTML逐字转义；图片只显示说明、不自动联网。链接由Rust现有url crate解析，仅显式HTTP(S)且无凭据、控制字符或反斜线的目标能在新窗口以noopener/noreferrer/no-referrer打开；相对路径、文件和应用scheme不能获得节点/本机能力。解析受1MiB、32768事件、48层栈边界约束，超限完整原文回退并提示，不截断公共正文；不提供数学/图表执行器、自动嵌入、文件跳转或所有Markdown扩展。
- 复制载荷在Rust AST中生成：回答为可读纯文本（段落空行、列表换行、表格tab），无作者、复制控件或工具内容；代码保留解析后字符串，CommonMark规范化换行不冒称原始字节复制。`CopyTextButton`只在用户手势调用Clipboard API；成功、pending、失败都明确，失败立即清旧成功图标，不走execCommand/选区后备路径。载荷变化与卸载使回调失效，A→B→A不复活旧反馈，不自动重试，也不声称能取消已发出的系统剪贴板写入。本批范围是回答及其代码，未重做registry工具/文件复制。
- 视觉沿确认原型的标题、正文16px/舒适紧凑节奏、代码头和内侧focus outline，长代码和表格仅局部滚动，手机复制按钮44px；新增Rust code背景token绑定而非另定调色盘。截图复核实证代码后的段落margin被清零，补断言先红后修到16px；全可选主题浏览器审计另复现Reddit浅色keyword对比度4.4757:1，由共享Rust新增code文字角色仅在低于4.5时回退正文色，全局accent不变，不降低门槛。
- 新增rich9项接现有runner和CI，总15→16套；历史结构/只读清单、HTML/链接/图片不联网、精确载荷与拒绝/缺失/同步异常、迟到focus/草稿选区/阅读位置/真实HTTP iframe、载荷A→B→A与会话切换、未闭合流式围栏→最终历史的正文/pre/anchor同DOM和局部横滚、上方富内容增高后的<2px锚点、双布局×明暗×1440/1024/390/320、所有5套主题明暗代码文字4.5:1、超深完整原文回退。剪贴板全部是隔离桩，禁止触及宿主；没有sleep猜协议完成。原settings14/scopes6/reading12及其余断言保留。
- 键盘默认门槛是Tab进入与双向离开/返回、focus-visible内侧outline计算值/截图、局部横滚范围及整窗无横溢出。原生ArrowRight在Chromium连续重排后偶发无动作，已留/tmp诊断；确认原型的 `verify-code-focus.cjs` 原本也只把它作为可选平台探针，本批沿用 `RPI_WORKBENCH_NATIVE_SCROLL_PROBE=1`，没有新增模拟快捷键或声称原生跨平台手验通过。不是四边PNG像素定量审计或所有平台键盘验收的替代。
- 最终双构建和16套浏览器全绿，共享Rust151→161（Markdown9+代码色1），WASM binding6项，三crate WASM严格Clippy、fmt/diff及CI YAML通过；lint69警告/0错误，无压制。Cargo.lock只新增解析库与直接依赖，不夹带离线resolver的无关Windows版本变化；NOTICE补直接依赖说明，完整传递依赖许可分发仍是发行边界。截图实际复核后仅留忽略artifacts/rich-answer，日志/tmp；保护服务/三张PNG不变，仅本地提交不推送，未运行远程CI或实际模型/Native验收。
- **整体目标仍active**：源码/diff与其他消息细节、未接通设置/真实协同、本机桥及Native/Android、刷新阅读持久化、真实模型长会话/各平台手动复验和正式发行仍未完成；本批富内容不等于完整UI交付。
