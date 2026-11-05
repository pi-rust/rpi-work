# rpi 会话资源能力验证

> 结论：**rpi 已具备部分底层能力，但当前 Work + rpi RPC 链路还不能直接交付头脑风暴中的完整会话资源配置。**工具切换、启动参数隔离、原生扩展组合和历史续用已实测；完整 Skills/Package 选择、RPC 资源重载、配置持久化及加载清单仍有缺口。
>
> 这是当前版本的能力核验，不是产品需求验收通过，也不是对所有 rpi 接入方式的否定。产品要求见[头脑风暴文档](../design/头脑风暴文档.md)。

## 1. 对象、方法与复现

初次验证日期：2026-10-30；补充复验：2026-10-31。

下表保留初次验证基线，不意味着机器当前仍安装该版本。补充复验时宿主和相邻 rpi 源码已更新到 0.15.0；下文原有源码行号对应初次核查的 0.14.0 commit，新增事实另标版本。

| 对象 | 初次验证版本/来源 |
| --- | --- |
| 已安装宿主 | `rpi 0.14.0`，Windows x64，实际启动 RPC 进程 |
| rpi 源码参考 | `D:/Projects/pi-rust`，`ee6ed10e0ed707383f7afc28e81655ff86a635fb`，workspace 版本 0.14.0，检查时工作区干净；未证明安装二进制由这个精确 commit 构建 |
| Work 当前依赖 | `crates/work-client/Cargo.toml:27–28` 的 `rpi-extcore = "=0.1.5"`，已核对 `Cargo.lock` 和本机 crates.io 缓存源码 |
| 相邻 extcore 源码 | `D:/Projects/rpi-package/packages/rpi-extcore` 为 0.2.1，不当作 Work 当前依赖 |
| 原生工具 fixture | 仓库自有 `examples/extension-demo` 源码与锁文件，SDK 0.8.3；复制到隔离目录后离线编译，没有升级或修改原示例 |
| 模型 | 仅 `127.0.0.1` 确定性 OpenAI-compatible SSE fixture；真实宿主、真实 DLL、真实会话落盘，不使用外部模型 |

复现命令：

```bash
node --check tools/review/verify-rpi-resources.mjs
node tools/review/verify-rpi-resources.mjs --native
```

脚本：[verify-rpi-resources.mjs](../../tools/review/verify-rpi-resources.mjs)。可通过 `RPI_WORK_SMOKE_BIN` 指定宿主；省略 `--native` 会跳过原生构建部分，不代表完整验证通过。依赖 Node.js、rpi、Rust 工具链和本机已缓存的锁定 Cargo 依赖；离线缺依赖时应报告未完成，不擅自联网或升级。

隔离措施：新建项目、HOME/USERPROFILE、APPDATA、agent 和会话目录；子进程只继承必要 OS 环境，显式指定本地模型和假 API key；`RPI_CODING_AGENT_DIR` 重定位、`RPI_OFFLINE=1`、`CARGO_NET_OFFLINE=true`。Cargo/Rustup 使用现有工具链及缓存，构建输出在隔离目录。测试不读取用户 Agent 凭据/会话，不安装到用户资源目录；内置工具只验证发送给模型的 schema，不执行文件/终端操作。

初次完整运行：**23 项断言通过、13 次本地模型请求、8 个 RPC 宿主全部正常退出，`cleanup=true`**。断言中包含“当前不支持/不自动恢复”的负向事实，不能解释为 23 项产品能力全部支持。

补充复验：**rpi 0.15.0，24 项断言通过、14 次本地模型请求、8 个宿主正常退出，`cleanup=true`**。此前 23 项行为再次通过，新增一项确认空工具选择的真实模型边界语义。相邻源码参考为 `fb68a45fb60c6c904b2836eb2f7d5cc9baec7948`（0.15.0，核查时干净），同样未证明二进制由该精确 commit 构建。**不将新增实测冒充为 0.14.0 的运行结果。**

本机证据（不进入 Git）：

- 汇总：`artifacts/rpi-resource-validation.json`（每次运行覆盖）。
- 初次 0.14.0 完整证据：`target/resource-validation-tlSi2a/result.json`。
- 补充 0.15.0 完整证据：`target/resource-validation-AY2QSF/result.json`；当前 `artifacts` 汇总对应此轮。
- 同目录 `a.json`、`b.json`、`a-resumed.json`、`native-a.json`、`native-switched.json` 等保存宿主原始帧；`*.stderr.log` 保存宿主输出；`commands.log` 保存真实构建和包命令结果。
- `result.json.requests` 保存本地 fixture 实际收到的 tools、系统提示词和消息；不是前端自己生成的成功状态。

## 2. 验证矩阵

| 产品依赖 | rpi 0.14.0 验证结果 | 当前 Work 接入判断 |
| --- | --- | --- |
| 同项目并发会话工具选择隔离 | **实测支持**：相同 cwd 的 A/B 同时发送请求，模型分别收到 `read` 和 `read+bash`；修改 A 不影响 B | 已有独立 Host 进程及 `set_active_tools`；不能据此推导完整资源配置已隔离 |
| 同项目不同 Skills/提示词 | **启动参数可区分，完整选集不成立**：`--skill` 和 `--system-prompt` 的独有标记不串会话；项目/全局资源仍自动合并 | 当前创建接口没有对应会话参数 |
| 一个主 Package + 独立工具扩展 | **底层组合实测支持**：项目 registry 只留 `validation-main`，同时通过 `--extension` 加载独立 DLL；两个工具均实际执行、计数为 2 | “主 Package”是产品分类，尚无对应的完整会话选择/应用接口；通用扩展管理仍 Unsupported |
| 空闲切换已注册 Tools | **实测支持**：下一轮模型 schema 改变，历史保留 | 已有入口；不是安装/卸载扩展，更不是沙箱 |
| 全部取消工具选择【0.15.0 补测】 | **空数组并非禁用**：RPC 返回 `activeTools=[]`，模型仍收到全部六工具 | 不得显示“0 项有效/全部关闭成功”；需要显式 default/only/none 语义 |
| 空闲修改 Skills/Package，原进程热应用 | **当前 stock RPC 缺入口**：新增 Skill 未自动发现；`execute_command /reload` 返回 unknown command；ready 未广告资源配置命令 | 不能用前端保存或调用 TUI 命令冒充已应用 |
| 换配置并保留历史 | **重启路径实测支持**：同 session ID 重开，更换 Skills/提示词，模型收到旧历史；另一组更换显式原生扩展集合后也续用旧历史 | 底层已有 reopen；资源参数透传、保存、错误恢复和应用确认尚未接入 |
| 资源配置跨重启恢复 | **不自动保证**：同 ID 重开后，工具从 `read+bash` 恢复为启动默认六工具；调用方重新设置后生效 | 需要保存选择并在新宿主执行前重新应用/核对，不能把历史恢复当成配置恢复 |
| 资源变化自动进入旧会话 | **本次未发生**：新增 Skill 只在新宿主进入模型提示词，旧宿主下一轮仍无该条目 | 不应宣称安装成功后旧会话立即具备新资源 |
| 已加载/可用/启用/使用的观测 | **Tools 部分可读，Skills 清单有缺口**：`get_available_tools` 有工具名，`get_state.activeTools` 有选择；没有对称 Skills 资源清单 RPC | 需要上游资源快照/诊断事实；不能靠扫描磁盘代替宿主实际状态 |
| 全局与项目 Skill 冲突 | **实测项目优先**：同名 Skill 使用项目描述；不同名全局 Skill 正常出现；`--project-only` 排除全局 | 可依照宿主结果展示，尚不能由现有 RPC 得到完整来源/冲突诊断 |
| 本机全局/项目安装与下载 | **部分核实**：项目 `package link/unlink`、真实加载/执行已实测；全局安装路径仅源码核查；网络下载未测 | Work 生产资源安装流程未因此获得支持；本轮不评估远程资源分发 |

## 3. 关键证据与边界

### 3.1 进程隔离不等于资源选集隔离

- extcore 0.1.5 的 `src/service.rs:507–562` 为新 session 建立 Host，`src/host.rs:310–348` 启动子进程；同一个 session ID 的活跃 Host 则复用（`src/service.rs:829–866`）。
- Work 的 `crates/work-client/src/local_workspace.rs:238–258` 按项目复用 Service。这**不是所有会话共用一个 rpi 进程**，但它们仍共享项目文件与资源目录。
- `--skill` 是追加资源路径，不是“只启用选中 Skills”。`session.rs:595–607` 先取项目/全局目录，再追加显式路径；`resource_dirs.rs:344–360` 还会加入所有项目 registry Package 的资源目录。
- `--local-only` 可让扩展仅按显式路径加载（`session.rs:2014–2035`），但不提供完整 Package/Skills 白名单。实测重开时 `work_demo_counter` 已从工具注册表消失，项目 Package 的 `MAIN_PACKAGE_SKILL` 仍进入提示词。

因此，可以利用原生启动参数建立不同的执行集合，但**尚不能把当前 CLI 等同于“一个主 Package + 精确多选 Skills/Tools”的完整会话配置契约**。不得通过反复覆盖项目共享 `.rpi/settings.json` 实现并发切换。

### 3.2 历史可以续用，资源配置必须另行保证

本次普通会话流程：

```text
A 启动：Skill A + 提示词 A → 两轮对话 → active tools = read,bash
  → 停止宿主
  → 同 session ID 启动：Skill B + 新提示词
  → get_messages 有旧消息，下一轮模型也收到旧历史
  → active tools 却恢复为 read,bash,edit,write,docs,powershell
  → 显式重新 set_active_tools 后，下一轮才恢复为 read,bash
```

原生扩展流程另外证明：保持项目和 session ID，重开时改用 `--local-only --extension <独立DLL>`，下一轮不再提供原主包工具、仍提供独立工具，同时继续收到此前原生工具对话历史；另一个并发宿主保留原包及独立的内存计数。

这只证明底层可行性，不是完整 Package 切换事务：旧对话/工具结果依然在上下文，Package 关联 Skills 也可能继续自动发现。真实模型面对旧工具结果、切换失败回滚、运行中拒绝修改等，未在本轮验收。

### 3.3 不要把 TUI reload 与 RPC 混为一谈

实际响应：

```json
{"command":"execute_command","success":false,"error":"unknown command: /reload"}
```

- `rpc/protocol.rs:120–150` 的 ready 命令没有 reload、set skills 或 set package。
- `modes.rs:805–855` 的 `execute_command` 只派发已注册的扩展命令，不派发 TUI 内置命令。
- `modes.rs:309–338` 的 RPC loop 接收 harness、模型目录、UI dialog mailbox 和扩展 registry snapshot；没有 TUI 的资源重载上下文/重载邮箱处理。不能凭插件调用 Reload 动作就宣称这条 RPC 链路完成重载。
- **库层并非没有动态资源能力**：`rpi-harness/src/agent_harness.rs:2254,2396,2407` 有 `set_tools`、`set_system_prompt`、`set_resources`；CLI `session.rs:1296` 有 `reload_extension_resources`，TUI 使用它。缺口在当前受支持、可观测的 RPC/适配契约，不应重写已有资源加载器。

### 3.4 Skills “未出现”不代表“未加载”，出现也不代表使用

首次探测发现 B 只启用 `bash` 时没有 Skills 列表。源码和后续实测确认这是 **read-tool gate**：`agent_harness.rs:3385–3402` 只在启用名为 `read` 的工具时注入 `<available_skills>`。重新启用 `read` 后，不重载资源也能恢复列表。

因此需要区分：

1. 文件已安装；
2. 宿主已发现/加载；
3. 当前选择使其对模型可见；
4. 模型本轮实际读取/调用。

本轮通过 fixture 证明提示词列表和工具 schema；未要求模型读取 Skill 正文，不能记为“Skill 已使用”。原生工具则通过真实执行事件及 DLL 内状态验证，不只是目录出现名字。

### 3.5 安装格式与来源仍需按实现核实

- 项目 registry 是 `.rpi/packages.json`，加载资源目录为 `.rpi/packages/<name>/{extensions,skills,prompts}`，见 `packages.rs:606–621`、`resource_dirs.rs:344–360`。
- 当前 `package link` 的源码路径 `packages.rs:125–191` 主要复制构建出的 cdylib 并写 registry，未见把源目录的 Skills/Prompts/Themes 一并复制的步骤。本轮 Package Skill 是**测试脚本明确写入已安装目录的 fixture**，仅验证宿主发现，不能证明安装器完整分发这些资源。
- 全局原生安装目标是 `<agent_dir>/extensions`，记录在 `<agent_dir>/native-packages.json`（`install.rs:342–379`）；本轮没有执行用户全局安装或网络下载。
- 不能仅凭文档中的 `settings.packages` 判定它能完成加载。当前参考源码的 `Settings` 有 `skillDirs/promptDirs/extensionDirs`，未见消费 `packages` 的字段；`settings.rs:1215` 是未知字段保留测试，不是包加载测试。实际项目包发现读取单独 registry。此项是源码核查，未对任意第三方 Package 格式做兼容性实测。
- 本轮没有新建包格式，也没有把“独立工具扩展”变成第二个产品主 Package。

### 3.6 空选工具并非关闭全部【0.15.0 补测】

实际执行 `set_active_tools({tools: []})` 后：`get_available_tools` 返回 `activeTools: []`，但本地模型 fixture 实际收到 `bash, docs, edit, powershell, read, write` 六个工具。没有执行这些内置工具，仅检查真实模型请求的 schema。

0.15.0 源码 `rpi-harness/src/agent_harness.rs:3368–3382` 的 `active_tools` 明确把空选择展开为全部已注册工具；初次 0.14.0 源码核查也见同语义，但本报告仅把此次补测计入 0.15.0 运行证据。

Work 当前资源面板以 `active_tools` 数组作勾选/计数，并直接提交修改后的数组（`apps/work-ui/src/features/resources/session_config.rs` 的工具变更逻辑）。因此存在“最后一项取消后界面显示空选，但实际模型仍可调用全部注册工具”的风险。**本轮没有改 UI 或宿主逻辑**，应列为工具配置窄闭环的优先修复和回归项：先明确空选降级提示，最终由上游提供可靠的 `none` 语义；不能用未知名称或虚构工具绕过。

## 4. Work 当前适配缺口

以下是接口/源码事实，不能由此次 CLI 实测自动补齐：

- extcore 0.1.5 的 `BackendConfig`（`src/service.rs:21–31`）、`CreateSessionRequest`（同文件 `486–489`）、`SessionOptions`（`src/host.rs:155–169`）没有 Package、Skills、扩展路径或系统提示词的会话资源字段。新建会话只传项目不够。
- Work `local_workspace.rs:883–903` 及 extcore `src/protocol.rs:4–32` 限制公共命令，不能用泛型 command 假装接通不存在的资源控制协议。extcore 的 Host 底层有 JSON 透传/扩展命令能力，但不产生宿主不具备的语义。
- Work 已有 `get_available_tools`、`get_state` 和 `set_active_tools`（`local_workspace.rs:1103–1179`），优先复用。
- `crates/work-client/src/extensions.rs:11–22` 仍返回默认关闭能力和 Unsupported。本次隔离 demo 的安装/执行不代表生产通用扩展管理、UI 扩展或资源应用确认已交付。

## 5. 对下一步设计的约束

**保留产品要求；把技术缺口显式列入实施前提，不降低为“改前端勾选即可”。**后续已整理[最小会话资源契约提案](../design/session-resource-contract.md)，区分当前能力与建议语义；其中名称不是正式 API。优先与 rpi 上游对齐：

1. **按会话的资源选择/启动配置**：精确有效的 Package/Skills/扩展集合与提示词；复用原生加载器，防止自动发现混入未选资源；区分默认工具、精确选择与禁用全部，不能把空数组直接解释成零工具。
2. **空闲应用路径**：直接复用现有库层 reload/setters，或受控重启同一会话；两者都要有忙碌校验、失败状态及可核实的应用结果。是否接受重启方案还需设计确认，不能直接当作产品已批准的实现。
3. **配置恢复**：会话选择与 transcript 分开保存，新宿主接收模型请求前重新应用并检查；不能恢复到更宽工具集后静默继续执行。
4. **资源观测**：宿主返回实际资源及来源/诊断，明确 loaded、active、model-visible、used 的区别；Skills 缺少可靠清单时显示未知/未接通，不伪造成功。
5. **安装闭环**：本机全局/项目作用域、独立原生工具、来源确认，以及 Package 附带非 DLL 资源的真实分发；网络下载、失败恢复与完整清单另行验收。

本轮没有修改业务代码、上游源码或现有示例依赖；仅新增验证脚本/报告，并将结论写回头脑风暴文档。ACP 资源配置、远程分发、跨节点协作、生产 UI 和真实外部模型表现不在此次验证范围。
