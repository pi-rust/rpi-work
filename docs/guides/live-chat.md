# 真实节点聊天

桌面是节点宿主：本机 RPC 与内嵌 HTTP/WebSocket 共用同一份 `LocalWorkspace`，Web/手机是访问入口。独立 `rpi-http` 不等于桌面 owner。旧迁移副本不自动搬回或合并。

## 本机启动

配置 Git ignored `.env.local` 的 `RPI_WORK_HTTP_PASSWORD`（兼容 `RPI_HTTP_PASSWORD`，勿提交实际密码），安装兼容 rpi 后运行：

```powershell
task dev
```

它构建 release WASM 与 `work-cli`，以前台无头 owner 在 **http://127.0.0.1:13031/** 直接提供 Web/API，有压缩与内容哈希缓存，不热刷新。首次缺少共享偏好时开启共享，已有关闭偏好则保留。Web 使用当前页面 origin 和宿主密码访问同一 owner。无头模式用 Ctrl+C 协作退出。

频繁修改 UI 用 `task dev:desktop`：桌面 owner + **http://127.0.0.1:14180/** debug Web 构建监听/刷新，API/实时通道代理到 `13031`。只需桌面用 `task desktop`。相同 data 只能选一条 owner 启动路径；`task node` 是独立 rpi HTTP 服务（默认 `13030`），不与 Work owner 共享，也不能推定支持 Work 全部协议。独立节点原有数据与已迁移副本继续保留。当前已经运行的旧桌面或 Web 代理不会因源码改变自动切换，不能擅自中断当前会话来启用新能力。

## 端口、二维码与手机

- `RPI_WORK_HTTP_PORT` 更改 owner 入口及 `task dev:desktop` 代理目标；`PORT` 只更改 debug Web 端口。
- 局域网可使用 `http://<电脑IP>:14180/` 的同源 Web 代理；桌面 API 可以继续仅回环监听。
- 直接提供桌面 Web 页面需配置显式监听、可达 origin 和已构建 assets 目录；见[桌面 HTTP 入口](desktop-http.md)。
- HTTPS 外部域名须由明确配置的同源反向代理提供，校验 Host/Origin，不删除认证；现有代理仍指向独立 `13030`时不会自动访问桌面，应由用户择时更改并验证。
- 普通分享码只包含地址，手机浏览器仍需密码；Android 原生 App 使用另发的 120 秒、单次授权码，核对节点后兑换。节点组件页可按需打开专用远程访问面板；监听配置保存后下次启动生效。
- 手机 HTTP 页面的相机需要安全上下文，普通 LAN HTTP 可使用相册；桌面浏览器移动模拟不能替代实体手机验收。

## 手动启动 Web

在 Work owner 已运行且入口 `13031` 可用时，另开终端启动 Web（不启动第二个 owner）：

```powershell
$env:RPI_API_PORT = "13031"
node tools/dev/dev-web.mjs --watch-build --debug-build
```

手动脚本仍可通过 `RPI_API_HOST` / `RPI_API_PORT` 代理其他明确选择的节点。默认值不代表其与桌面相同；检查实际目标，避免读到 standalone 的会话库。自定义域名用 `RPI_UI_ALLOWED_HOSTS` 显式配置。

## 对话生命周期

- 项目/会话目录在可见窗口中每 3 秒刷新，focus 或恢复 visible 时立即刷新（在途请求完成后补刷），隐藏时暂停；失败保留目录并自动重试。节点与连接 epoch 隔离迟到结果，并检查本端并发目录修改，避免覆盖改名或归档。目录刷新不等于历史全量同步，当前会话使用协商后的实时通道及历史校准。
- WebSocket `subscribed` 表示订阅在线，不能使空闲会话进入运行状态。
- `prompt` 接受后由 `agent_start` / `operation_accepted` 驱动运行，文本增量和工具事件都在同一对话内显示。
- rpi的用户消息可能只写入持久化历史，不另发消息事件。客户端在运行开始读取已落盘的用户消息，在 `agent_settled` 后对齐正式历史与流式消息ID；读取最多等待5秒，不推进事件游标、不重发prompt。
- 同一次消息的临时ID和最终ID、工具结果的start/end事件不会变成重复记录。历史中的工具调用、实时执行状态和最终结果按 `toolCallId` 合并到一张卡片。
- 仅 `agent_settled` 决定回合结果；`agent_end` 仍保持等待结算。结算后状态栏显示实际完成/失败状态。

## 复验

无模型调用的代理边界检查：

```powershell
node tests/e2e/verify-dev-proxy.cjs
```

真实模型检查需要明确传入 `--run-model`，会在当前工作区对应项目中创建或复用专门的验证会话，实际调用模型两轮，其中一轮仅开放read工具读取README。结束时恢复该验证会话原有的工具集合，不修改其他会话。验证记录保存在本机 `target/live-chat-session.json`；不会自动进入普通CI。

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:14180/'
$env:RPI_WORK_NODE_URL = 'http://127.0.0.1:13031' # 明确使用待验证的 Work owner
# 从本地服务环境JSON读取RPI_HTTP_PASSWORD或RPI_WEB_PASSWORD，避免把密码写进命令或报告。
$env:RPI_WORK_NODE_ENV_FILE = 'C:/path/to/private-node-environment.json'
# 可选：provider/model；省略则使用该验证会话当前模型。
$env:RPI_WORK_REVIEW_MODEL = 'provider/model'
node tests/e2e/verify-live-chat.cjs --run-model
```

脚本使用隔离浏览器，真实请求不做route拦截，核对节点认证、会话模型切换、WebSocket文本增量、实际工具完成事件、服务端结算、用户/助手/工具卡片与正式历史的数量，以及390像素布局。报告与截图输出到被Git忽略的 `artifacts/live-chat-*`，不包含节点密码、模型密钥或Cookie。当前脚本依赖项目审查环境中的Playwright/Chrome，可通过 `CHROME_PATH` 指定Chrome。

此次实际验证模型为 `huoshan-copy/deepseek-v4-flash`。统计遵循节点返回；该节点在这次检查中报告token/费用为0，客户端不会用上下文估算值冒充实际计费数据。
