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
| 阅读列与消息层次 | 保留真实投影，迁入原型阅读/经典布局、完成折叠及工具交互 | 流式、失败、历史恢复、滚动锚点、草稿/选区保留 |
| Composer、模型、思考、队列与审批 | 第五批已接通并本地验收：canonical绑定、共享Rust准入、ACK/结算分离、真实选项单次回复及typed回执只读恢复；整体UI迁移仍未完成 | IME、accepted 未确认、迟到回调、能力变化、附件边界 |
| 右侧浏览器 | 首批施工：真实 HTTP(S) 页面、多标签、地址与历史、刷新、视口 | 隔离的真实 HTTP 页面；拒绝危险协议/凭据；不传节点密钥；隐藏不重载 |
| 源码、目录与 diff | 已有 NodeConnection 文件/Git 调用；布局与原型不同 | ticket/revision、长行固定控件、主题对比度、仅能力允许的写入 |
| 居中设置与本地偏好 | 现有 SettingsPage 是旧视觉结构 | 搜索、焦点恢复、原型布局；本地偏好与执行配置分离 |
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
