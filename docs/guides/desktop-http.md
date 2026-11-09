# 通过 HTTP 访问桌面节点

这是源码新增能力，不会为正在运行的旧桌面动态添加监听。请自行选择合适时间启动新构建；不要为验证而终止正在承载会话的桌面。

## 同一个 owner

桌面只创建一次 `LocalWorkspace`。本机 facade 和 Work HTTP/WebSocket adapter 持有同一个 `Arc`，共用项目、registry、会话、运行任务、历史和 ACP manager。独立 `rpi-http`（通常 13030）仍是另一个节点，不会被自动接管、关闭或合并。

- `task desktop`：启动桌面 owner，内嵌认证 API 默认 `0.0.0.0:13031`；首次没有共享偏好时启用局域网共享，仍需密码或一次性扫码授权。
- `task dev`：构建 release Web 并启动无头 owner，在 `13031` 直接托管，不热刷新。
- `task dev:desktop`：并行启动桌面 owner 与 `14180` debug Web 构建监听/刷新，API/实时通道同源代理到 `13031`。相同 data 不可另开 `task desktop` 或其他 owner。
- `task node`：显式启动独立 standalone 节点，不是桌面工作空间。
- `work-cli serve` / `work-ui --headless`：复用 `work-host` 启动相同 owner，不创建窗口。无头要求显式非空密码；启动和正常停止见[无头节点与统一 CLI](headless-node.md)。
- `work-ui --connect HTTP(S)_ORIGIN` 或显式远程 `RPI_WORK_NODE_URL`：只连接既有节点，不创建本机 owner；关闭窗口不停止该节点。默认拥有本机 owner 的桌面在切换远程视图后，仍可点“使用这台电脑”返回自己的 owner。

端口被占用时，GUI 报告桌面 HTTP 不可用，本机 RPC 不因此失效；无头模式则清理宿主并非零退出，不报告 ready。两者均不连接占用端口的服务、不终止它、不自动选择另一端口。

## 宿主 Web 登录与手机 App 授权

Web 与桌面是平级客户端：默认使用当前节点，也可以主动连接其他节点。桌面直接提供 Web assets，或由同源代理提供 Web、桌面提供 API/WebSocket 时，Web 默认访问同一个桌面 owner，不是永久绑定。已有有效 Cookie 直接进入；首次或登录失效时显示 **登录宿主**。不要求为默认节点另填地址或点击连接，登录前不挂载工作台、不读取项目/session。认证后在连接设置点 **连接其他电脑**，可进入其他节点自己的 Web 页面独立登录，再用 **返回入口节点** 返回；不要求将当前 Web 改成 standalone。旧历史不得在启动失败时自动替代当前宿主。手机浏览器仍是 Web，移动原生客户端不托管本机节点、只连接外部节点。详见[客户端地位与默认节点](../design/node-session-ownership.md#11-客户端地位与默认节点产品约定)。

Web 的跨节点适配是同源页面导航，不是跨源 API 直连或凭据代理。目标须有 hosted 声明及 Web assets／同源 Web 代理；仅 API 的节点需先配置 Web 入口。返回地址是经校验的手动导航提示，不是授权，不会自动跳转。当前节点认证失效可点 **重新登录当前节点** 回到自己的宿主门禁。限制、Cookie host 边界及隔离测试见[宿主 Web 切换节点](node-connection.md#宿主-web-切换节点)。

宿主通过 `GET /.well-known/rpi-work-entry` 声明入口：配置了 assets，或通过有效 `RPI_WORK_HTTP_WEB_PORT` 明确声明同源 Web 代理时返回 hosted；纯 API 且未声明代理仍返回404。`dev-web` 精确转发该声明，默认 hosted；上游404被视为配置/版本缺失并返回502，其他故障或无效元数据也不能降级到历史节点。显式独立静态 Web 才对该路径返回真实404，保留节点连接流程，不能用 HTML fallback 冒充声明。

**Android 原生 App 使用另一种认证入口**：宿主桌面或已登录 Web 点“手机 App 扫码授权”，App 扫码、核对地址、点“授权连接”，无需输入密码。随机票据 120 秒有效、单次兑换，关闭/重开共享使旧票据和旧登录失效；只在内存保留票据摘要。普通地址分享码不能代替授权码。授权码是短期 bearer 凭据，只向可信手机展示；已签发的码关闭弹窗后仍可能在过期前有效。没有逐设备权限、设备审批或永久配对。

## 是否允许共享

首次缺失 `sharing.json` 时**默认开启局域网共享**；已保存的关闭选择始终保留，损坏或不可读的偏好仍按关闭处理并提示。在宿主桌面 **设置 → 执行节点 → 这台电脑的共享 → 允许共享这台电脑** 可关闭；选择会保存在当前 data 的 `sharing.json`，不含密码。桌面正在查看其他远程节点时仍能控制自己这台电脑的共享，二者互不影响；Web/手机没有该宿主控制权限。

- 关闭时，HTTP 工作空间 API（包括登录）返回 `403 / sharing_disabled`，已有 WebSocket 立即断开，旧 Cookie 作废。重新开启后须重新登录。
- **关闭共享不停止本机任务、不关闭 workspace、不影响桌面连接其他节点。** 已经接受的命令不自动撤回，已被客户端读取的内容也无法收回。
- listener/executor 可以保持存活：health 只报告存活和共享状态，静态客户端资源不包含节点数据；不能把端口仍可连接误判为项目/会话仍可访问。
- 保存失败不显示虚假成功；仅偏好缺失采用首次开启策略，显式关闭不会被启动逻辑覆盖。开关不会自动更改监听网卡、防火墙、Host 白名单或反向代理。
- 当前运行的旧桌面没有该开关；需由用户择时启用新构建，不要因此中断活跃会话。

会话导航顶部及右侧 **任务上下文 → 执行环境** 显示“本机工作空间”（电脑图标）或“远程工作空间”（云图标），并显示节点地址；远程包含局域网，不代表一定部署于云端。节点位置与在线状态分开显示。

## 配置

`task` 读取 Git ignored `.env.local`；直接启动二进制时使用进程环境变量。

| 环境变量 | 默认/用途 |
| --- | --- |
| `RPI_WORK_HTTP_BIND` | `0.0.0.0`，默认监听本机 IPv4 网卡以供局域网连接；显式 `127.0.0.1` / 指定网卡仍优先，配置无效不回退 |
| `RPI_WORK_HTTP_PORT` | `13031`；`task dev` 将同值传给 Web 代理 |
| `RPI_WORK_HTTP_PASSWORD` | HTTP 登录密码；未设置时兼容 `RPI_HTTP_PASSWORD`；GUI 两者均无时生成随机密码，无头必须显式非空；显式空值拒绝启动 HTTP |
| `RPI_WORK_HTTP_ALLOWED_HOSTS` | 额外允许的 Host authority，逗号分隔，非默认端口必须包含端口 |
| `RPI_WORK_HTTP_PUBLIC_URL` | 可选，手机可达的 HTTP(S) origin；优先作为二维码地址，自动加入 Host 白名单，不含密码/参数/片段 |
| `RPI_WORK_HTTP_WEB_PORT` | 可选，已配置同源 Web 代理的端口；`task dev` 自动传入 Web 端口（默认 `14180`），用于生成局域网二维码；不启动代理或改变监听 |
| `RPI_WORK_HTTP_ASSETS` | 可选，已构建 Web bundle 目录（含 `index.html`）；与 API 同源提供资源，目录须存在 |

桌面“执行节点”展示实际监听及共享状态；开启共享后才显示本机二维码和密码查看入口。未配置密码时随机密码只保留在本次进程内；不写日志、二维码或节点历史。HTTP Cookie 有效期 12 小时，桌面退出即失效。保留 Host/Origin、请求标记、登录限流；不开放通配 CORS。共享密码及一次性扫码授权都授予整个节点的访问权限，不是 ACL 或逐设备权限。

### 默认端口（均大于 10000）

| 入口 | 默认端口 | 覆盖方式 |
| --- | --- | --- |
| 桌面／无头认证 API、CLI 默认目标 | `13031` | 宿主 `RPI_WORK_HTTP_PORT`；客户端 `--url` / `RPI_WORK_URL` |
| 独立 standalone 节点 | `13030` | `RPI_API_PORT`；CLI 访问须显式 `--url` / `RPI_WORK_URL`，上游可执行文件须显式 `--port 13030` |
| Web 开发预览（含直接运行 dev-web） | `14180` | `PORT` |
| 审查 / 模块预览 | `14182` | `PORT` |
| 手动静态原型示例 | `14173` | `python -m http.server <port>` |

`task dev` 的 Web 代理默认访问桌面 `13031`；直接运行 `tools/dev/dev-web.mjs` 也默认访问桌面 `13031`，不再默认指向独立节点。代理端 `RPI_UI_ENTRY_MODE` 缺省为 `hosted`，可选 `standalone` 才使用独立 Web 入口（此时显式设置目标 `RPI_API_PORT`，例如独立节点13030）；非法值拒绝启动。`RPI_UI_ASSETS` 可指向已构建候选目录，缺省仍为 `target/parity-web`；自定义目录不能与 `--watch-build` 同用，避免 watcher 意外写入另一构建目录。桌面端仍须有 assets 或有效 `RPI_WORK_HTTP_WEB_PORT` 的明确宿主声明。

审查工具的原生 CDP 连接示例使用 `19223`，不会自动开启调试监听。

这些仅为缺省值，不限制显式设置的小端口，也不重写 `.env.local`、已保存节点地址或正在运行的服务。升级后须自行择时重启；已有代理、防火墙规则和手机上的旧连接地址需按实际配置调整。测试中的 `port=0` 仍交给系统分配随机空闲端口，不固定化。外部上游程序的默认端口不由本仓库更改。

### 自动生成局域网二维码

桌面本机 owner 就绪、HTTP 共享已启用且有有效 App 地址后，**无需进入设置，启动时自动尝试一次 App 授权二维码**。同一个组件在 workspace 生命周期内常驻，设置页与移动布局只请求打开它，不另建签发实例。关闭、失败、过期、重连或切换页面不会自动续签；可从“设置 → 执行节点 → 手机 App 扫码授权”显式重开或重新生成。首次显示遇到其他已打开的弹窗时不叠加，也不后台续发。普通浏览器地址码仍需主动打开，不能代替 Android App 授权。

App 与浏览器地址分开：App 只需要认证 API，因此无 assets 的默认桌面也可生成 App 授权；浏览器分享地址仍需 Web assets、同源代理或显式配置入口。

默认地址按优先级选择：
1. 显式 `RPI_WORK_HTTP_PUBLIC_URL`。
2. 配置 `RPI_WORK_HTTP_WEB_PORT` 时，使用默认路由所选的本机私有 IPv4 与代理端口。`task dev:desktop` 默认使用 `http://<局域网IP>:14180/`。
3. 直接 API 模式：HTTP 已监听非回环时，使用绑定的私有 IPv4（通配 IPv4 绑定时使用默认路由地址）及 HTTP 端口作为 App 授权地址；只有另有 `RPI_WORK_HTTP_ASSETS` 时才同时作为浏览器入口。只把确切 authority 加入 Host 白名单，通配 IPv4 监听另保留 `127.0.0.1` / `localhost` authority 供本机代理使用，不开放通配 Host。

检测只做本机 UDP 路由选择，不发送数据、不扫描网络。多网卡/VPN 可能选到非期望网卡，可配置 `RPI_WORK_HTTP_PUBLIC_URL`；普通 Web 分享仍可编辑“对方可访问的节点地址”。更换分享地址不切换 owner 或修改监听。无可用私有 IPv4、HTTP 启动失败、共享关闭或显式仅回环且无代理时不伪造可连接二维码；IPv6/公网入口可用显式 public URL。签发前对候选入口做无凭据 health 探测与 node ID 校验，只有验证通过才签发；本机探测不代表已穿过手机侧路由或防火墙。**不会自动更改防火墙，LAN HTTP 仅用于可信网络；需要加密时配置 HTTPS 入口。**

### 开发环境手机访问（推荐同源代理）

1. 使用 `task dev:desktop`，在桌面确认共享已开启（已有关闭偏好需手动打开），确认 Web 代理目标为桌面 `13031`，不是旧 standalone `13030`。
2. 默认 App 授权入口使用 `http://<电脑局域网IP>:14180/`；普通浏览器地址码可从“手机连接这台电脑”打开。多网卡或自定义入口可显式配置 public URL。开发代理必须实际启动；若希望 API 只监听回环，可显式设置 `RPI_WORK_HTTP_BIND=127.0.0.1`。
3. 默认开发 Web 与桌面直接托管 Web 都只在必要时显示宿主密码登录，随后自动加载当前节点会话，不需要为默认节点另选地址或再点连接。显式 `RPI_UI_ENTRY_MODE=standalone` 没有已确定的宿主默认节点，首次需自行选择目标；这与hosted Web进入后可主动切换其他节点是两回事。不填写手机自己的 localhost，也不要把没有 assets 的 API `13031` 当作 Web 页面；代理提供页面时应打开代理地址。
4. 若使用自定义 HTTPS 反向代理，配置代理的 Host 白名单，保持 API 与 Web 同源；不要关闭安全校验。

### 桌面直接提供 Web

先构建 Web bundle，再为**下一次启动**配置（示例只表达配置，不自动修改防火墙或启动进程）：

```dotenv
RPI_WORK_HTTP_BIND=0.0.0.0
RPI_WORK_HTTP_PORT=13031
RPI_WORK_HTTP_PUBLIC_URL=http://<电脑局域网IP>:13031/
RPI_WORK_HTTP_ASSETS=D:/path/to/rpi-work/target/parity-web
# RPI_WORK_HTTP_PASSWORD 在本机另行设置，禁止提交实际密码。
```

HTTP 页面的手机相机通常需要 HTTPS；局域网 HTTP 可使用相册。普通地址二维码只传递地址，不含秘密；专用 App 授权二维码含短期单次票据，二者不要混淆。填写地址不会扩大监听范围。未启动 HTTP 时不提供本机分享二维码。

## 生命周期与验证边界

listener、WebSocket 断开或切换页面不停止会话。桌面退出依次停止 HTTP 接收并等待在途调用、shutdown 现有 workspace children、结束 HTTP executor、释放数据锁。

`cargo test -p work-node-service --locked` 使用临时目录、随机端口、协议 fixture。测试比较本机与 HTTP 的会话 ID、元数据、事件序号和宿主进程 PID，验证不是两份文件副本；也覆盖认证、Host/Origin、请求标记、限流、端口冲突及静态资源目录边界。它不代表当前旧 GUI 已升级或实体手机已验收。

稳定 node ID 保存在桌面 data 的 `node-id`，HTTP health 返回 `nodeId`。客户端缓存/凭据目前仍按地址隔离；不据此自动合并不同地址的历史，也不移动已存在的迁移副本。

默认 Web 入口专项复验：`tests/e2e/verify-desktop-owner-http.cjs` 分别在直接托管与 `WORK_OWNER_PROXY=1` 的 API-only owner + 真实 dev-web 代理模式运行；`verify-desktop-web-proxy.cjs` 验证故障不回退、精确 Host/Origin、Cookie/WebSocket及显式standalone。全部使用隔离 fixture／随机端口。报告见 `artifacts/desktop-web-entry/REPORT.md`；源码修复不等于当前线上服务已经更新。
