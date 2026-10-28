# 真实节点聊天

正式客户端通过 `work-client` 调用 rpi-http / rpi-web 的 HTTP API，并订阅当前会话的 SSE。Provider和模型密钥由节点宿主管理；浏览器只使用节点密码登录。

## 本机启动

先启动已经配置好模型的 rpi-http 节点，默认地址为 `http://127.0.0.1:3030`。构建并运行 Web 客户端：

```powershell
pwsh -File scripts/build-ui-web.ps1
$env:PORT = '4180'
$env:RPI_UI_HOST = '127.0.0.1'
node scripts/dev-web.mjs
```

打开 http://127.0.0.1:4180/ ，进入“设置 → 执行节点”，连接本机并输入已有节点密码。登录后返回会话，从项目侧栏打开或新建任务。输入栏可选择节点实际提供的模型和思考等级，然后发送消息。

Web会话可通过 `/?session=<sessionId>` 直接打开：连接的是该页面的服务节点，认证有效时进入会话，需要密码时先打开节点登录页，登录成功后进入目标会话。链接不携带密码，也不会跳到本设备记住的其他节点。

`RPI_API_HOST` / `RPI_API_PORT` 可指定被代理的节点，`--watch-build` 可在源码变化后重新构建。默认API端口3030。开发服务器校验请求Host和Origin，仅把已接受的本界面来源转换为节点来源，不删除上游认证与请求标记。自定义域名需要在 `RPI_UI_ALLOWED_HOSTS` 显式配置（逗号分隔，包含实际非默认端口）。

本机验证只绑定回环地址。手机真机访问需要另行配置可达的监听地址、节点Host和网络入口；移动视口截图不能替代手机网络和相机验证。

## 对话生命周期

- SSE `ready` 表示订阅在线，不能使空闲会话进入运行状态。
- `prompt` 接受后由 `agent_start` / `operation_accepted` 驱动运行，文本增量和工具事件都在同一对话内显示。
- rpi的用户消息可能只写入持久化历史，不另发消息事件。客户端在运行开始读取已落盘的用户消息，在 `agent_settled` 后对齐正式历史与流式消息ID；读取最多等待5秒，不推进SSE游标、不重发prompt。
- 同一次消息的临时ID和最终ID、工具结果的start/end事件不会变成重复记录。历史中的工具调用、实时执行状态和最终结果按 `toolCallId` 合并到一张卡片。
- 仅 `agent_settled` 决定回合结果；`agent_end` 仍保持等待结算。结算后状态栏显示实际完成/失败状态。

## 复验

无模型调用的代理边界检查：

```powershell
node scripts/verify-dev-proxy.cjs
```

真实模型检查需要明确传入 `--run-model`，会在当前工作区对应项目中创建或复用专门的验证会话，实际调用模型两轮，其中一轮仅开放read工具读取README。结束时恢复该验证会话原有的工具集合，不修改其他会话。验证记录保存在本机 `target/live-chat-session.json`；不会自动进入普通CI。

```powershell
$env:WORK_UI_URL = 'http://127.0.0.1:4180/'
$env:RPI_WORK_NODE_URL = 'http://127.0.0.1:3030'
# 从本地服务环境JSON读取RPI_HTTP_PASSWORD或RPI_WEB_PASSWORD，避免把密码写进命令或报告。
$env:RPI_WORK_NODE_ENV_FILE = 'C:/path/to/private-node-environment.json'
# 可选：provider/model；省略则使用该验证会话当前模型。
$env:RPI_WORK_REVIEW_MODEL = 'provider/model'
node scripts/verify-live-chat.cjs --run-model
```

脚本使用隔离浏览器，真实请求不做route拦截，核对节点认证、会话模型切换、SSE文本增量、实际工具完成事件、服务端结算、用户/助手/工具卡片与正式历史的数量，以及390像素布局。报告与截图输出到被Git忽略的 `artifacts/live-chat-*`，不包含节点密码、模型密钥或Cookie。当前脚本依赖项目审查环境中的Playwright/Chrome，可通过 `CHROME_PATH` 指定Chrome。

此次实际验证模型为 `huoshan-copy/deepseek-v4-flash`。统计遵循节点返回；该节点在这次检查中报告token/费用为0，客户端不会用上下文估算值冒充实际计费数据。
