# 无头节点与统一 CLI

当前首版复用桌面现有 owner，不是第二套节点实现。`work-host` 不依赖 Dioxus/WebView；`work-cli serve` 与桌面 `--headless` 调用同一宿主。本文只描述源码能力，不代表正在运行的桌面已升级，也不自动部署或安装系统服务。

## 三种启动方式

| 方式 | owner 与退出行为 |
| --- | --- |
| `work-ui` | 默认启动本机 owner 并打开窗口；窗口退出时停止自己拥有的任务与子进程 |
| `work-cli serve` / `work-ui --headless` | 前台常驻同一个本机 owner，不创建窗口；停止宿主才停止其任务 |
| `work-ui --connect http://127.0.0.1:13031/` | 纯节点客户端；不解析本机 rpi 路径、不打开本机数据锁、不另建 owner；关闭窗口不停止外部节点 |

桌面也可通过 `RPI_WORK_NODE_URL` 显式选择 HTTP(S) 节点；`--connect` 优先。显式 `rpi-work://local` 仍表示启动本机 owner。默认 GUI 保留最近成功节点选择；显式远程启动失败不会创建本机 owner 兜底。非法目标或启动参数在启动节点前报错。

**同一数据目录只能有一个 owner。** 已运行无头节点时，不再打开另一个默认 owner；用 `--connect` 连接并独立认证。不会根据占用端口自动接管服务、复用其认证或认定它就是当前节点。不同数据目录可以运行独立节点，但它们不会自动同步。

## 启动无头节点

先正常构建：

```shell
cargo build --locked -p work-cli
# 不需要构建 work-ui；适合没有图形库/WebView 的服务器。
```

通过进程环境/可信凭据管理器安全注入 `RPI_WORK_HTTP_PASSWORD`，再运行：

```shell
work-cli serve
# 源码开发方式：
cargo run --locked -p work-cli -- serve
# 已安装桌面二进制也支持：
work-ui --headless
```

- 无头模式**必须有显式非空密码**：`RPI_WORK_HTTP_PASSWORD` 优先，未设置时兼容 `RPI_HTTP_PASSWORD`。不接受命令行密码，不生成一个无人可查看的随机密码；缺失时在创建数据目录前失败。GUI 原有随机密码行为不变。
- `RPI_WORK_RPI_BIN` 指定执行宿主；未指定时沿用桌面的 sibling/PATH 查找。`RPI_WORK_DATA_DIR` 沿用原平台目录，**不新增 headless 专用数据库**。对同一 data 使用同一 `<data>/node.lock`、`rpc/`、`node-id` 和 `sharing.json`。
- HTTP 配置复用 [桌面 HTTP 配置](desktop-http.md#配置)。默认仍是 `0.0.0.0:13031`，首次没有偏好时开启共享；这是既有桌面策略，不是无头新增的宽松模式。仅本机使用建议显式 `RPI_WORK_HTTP_BIND=127.0.0.1`；远程明文 HTTP 仅用于可信网络，公网配置 HTTPS 同源入口。
- 不自动配置防火墙、代理或 Web assets。API-only 不等于可直接用浏览器打开；需 `RPI_WORK_HTTP_ASSETS` 或已配置的同源 Web 代理声明。`RPI_WORK_HTTP_WEB_PORT` 只声明既有代理，不启动代理进程。
- HTTP 绑定/配置失败时，无头模式清理 owner 并以非零状态退出；不会输出成功 ready，也不会连接占用端口的其他服务。GUI 仍保留 HTTP 失败而本机 RPC 可用的既有行为。

启动成功向 stdout 输出一行 JSON：

```json
{"ready":true,"address":"http://127.0.0.1:13031/","nodeId":"持久节点标识","sharingEnabled":true,"webHosted":false}
```

`address` 是实际绑定 origin，通配地址不是远程可达性承诺。此行不含密码、Cookie、票据或用户会话。`ready` 只表示宿主 HTTP 已就绪；保存了共享关闭偏好、或偏好损坏时仍可能 `sharingEnabled:false`，登录/工作空间 API 返回 403。不会为了 CLI 可连而覆盖关闭偏好。需要恢复时先停止 owner，再通过可信本机管理流程恢复 `sharing.json`（只有 `allow_sharing`，没有密码）；不要改文件后误以为运行进程会自动重载。

Ready 发布使用原生非阻塞管道写入，准备/重试/部分写入都受同一个停止信号与 **15 秒发布期限**监督；不创建后台 writer，也不使用 Tokio 的阻塞 stdout worker。消费者必须等待完整 LF 行，取消/超时留下的前缀不能视为 ready。偏好损坏时可额外包含固定、非秘密的 `warning` 字段，不依赖 stderr 警告。

发布失败后先清理 owner 再以非零状态退出；致命 stderr 诊断只做一次尽力非阻塞写入，满/关闭的 stderr 可以丢失诊断，但不能阻止退出。普通文件/设备的同步 I/O 卡顿不属于此管道背压保证。

## 正常停止

- 控制台 `Ctrl+C`；Unix 额外支持 `SIGTERM`。
- 明确使用 `work-cli serve --shutdown-stdin` 或 `work-ui --headless --shutdown-stdin` 时，上级监督进程关闭 stdin/发送 EOF 触发正常停止；输入内容不是控制命令。
- 默认忽略 stdin EOF，不能把 systemd 等没有交互 stdin 的环境误判为停止请求。没有远程 HTTP shutdown 接口。

停止顺序固定为：停止 HTTP 接收并 drain 已接受请求 → 等待同一 workspace 的子进程退出 → 释放 HTTP executor → 释放数据锁。最多等待 10 秒；若不能确认清理完成，返回错误并保留资源/锁直到清理完成或进程退出，不把超时伪装成成功。强杀、机器断电和 OS 级崩溃不能保证协作清理；外部服务管理器应给足停止时间，并管理进程树。

本版是前台宿主，不安装 systemd/launchd/Windows Service，不自动后台化、开机自启、重启或执行日志轮转；这些属于后续运维集成。

## CLI 默认连接现有节点

CLI 默认 HTTP，地址为 `http://127.0.0.1:13031/`。`--url` / `--node` 覆盖 `RPI_WORK_URL`，只接受不含凭据、路径、查询和片段的 HTTP(S) origin。显式独立 `rpi-http` 节点仍可使用 `--url http://127.0.0.1:13030/`，不会与本机 owner 合并。

实际使用的 URL/transport 环境变量或启动参数无法表示为 UTF-8 时，在读取密码、连接或启动 Agent 前脱敏拒绝，不把坏值当作“未设置”后连接默认节点。显式目标覆盖未使用的 URL 环境值；帮助与 `serve` 不校验客户端环境。

客户端密码使用 **`RPI_WORK_PASSWORD`** 或 `--password-stdin`。stdin 优先，要求单行 UTF-8、含换行最多 4096 bytes，输入后 EOF，15 秒内完成；密码不会出现在错误、argv 或持久文件中。**每次命令独立认证，Cookie 仅本次进程内存有效；`auth` 不会保存下一次命令的登录，`logout` 也只注销本次凭据，不注销其他客户端。** 不把 `required:false` 当成已认证。

```shell
# 预先安全注入 RPI_WORK_PASSWORD。不要把真实密码写入 shell 历史。
work-cli --help
work-cli auth
work-cli projects
work-cli project-create "Demo" "/absolute/project/path"
work-cli session-create PROJECT_ID
work-cli session-rename SESSION_ID "New title"
work-cli prompt SESSION_ID "Hello"
work-cli state SESSION_ID
work-cli history SESSION_ID
work-cli watch SESSION_ID
work-cli abort SESSION_ID
work-cli dialog-reply SESSION_ID REQUEST_ID 'true'
work-cli --node https://node.example/ sessions
```

还支持 `health`、`capabilities`、`folders [PATH]`、`sessions`、`models SESSION_ID`、`tools SESSION_ID`。项目创建默认 `initGit=false`。会话创建/执行/对话回答取决于目标节点和绑定会话的实际能力，不新增 raw fallback，也不为协议 fixture 伪造成功。RPC 与 ACP 的回答校验沿用各自节点实现；通用 JSON 参数不绕过 ACP 的 outstanding-request、generation 和选项校验。

stdout 为 JSON（`watch` 为 JSONL），错误为 stderr/非零状态；无参数/帮助/非法命令在连接或启动 Agent 前处理。认证与单次操作有 15 秒 deadline，`watch` 持续等待，可 `Ctrl+C` 仅断开订阅，**不 abort 任务**。`prompt` 输出受理 ACK，不表示运行已结算；写操作超时为结果未知，先核对节点状态，不能自动重发。普通 CLI 退出不会停止节点 owner 或它拥有的任务。

### 显式原生诊断兼容

旧的直接启动 RPC/ACP Agent 的命令仅在明确配置 transport 时保留，不是统一节点客户端的默认通道：

```shell
RPI_WORK_TRANSPORT=rpc work-cli agent-info
RPI_WORK_TRANSPORT=rpc work-cli state
RPI_WORK_TRANSPORT=acp work-cli load-session SESSION_ID
```

PowerShell 用 `$env:RPI_WORK_TRANSPORT='rpc'` 等价设置；不再需要直连时移除该变量。HTTP 的 `state/history/models/tools/prompt` 有会话 ID 参数，与旧 native 诊断语法不同；请看 `--help`。未知 transport 拒绝执行，不悄悄回退 RPC。`serve` 是显式宿主命令，不受旧客户端 transport 变量改变。

## 验证范围

[无头候选报告](../../artifacts/headless-node/REPORT.md)记录本次源码、二进制与隔离验证。回归入口 `tests/e2e/verify-headless-node.py` 使用临时 data/home、随机 loopback 端口和协议 fixture，不读取真实会话、不调用真实模型或工具。Windows 协作 EOF 退出、跨入口重开同一 data 与客户端分离验证，不能代替 Linux 实机信号测试、原生 GUI 交互、系统服务安装或 Android 真机验收。
