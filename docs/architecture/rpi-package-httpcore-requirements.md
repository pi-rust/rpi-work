# rpi-package HTTP Core 能力需求

> 状态：Work 与 `rpi-package` 的架构对接需求草案
>
> 本文说明 `rpi-package` 中的 `rpi-httpcore`（或后续拆出的通用 HTTP transport crate）需要提供的能力，目标是让 Work 复用通用 HTTP/实时连接生命周期机制，同时保留 Work 自己的节点协议、`LocalWorkspace` 和组件系统。

## 1. 背景与目标

当前存在两套相近的 HTTP 实现：

- `rpi-package/packages/rpi-httpcore`：面向 `rpi_extcore::service::Service` 的标准 rpi HTTP/SSE API；
- `rpi-work/crates/transport/work-http-runtime` 与 `work-node-service`：面向 Work `LocalWorkspace` 的 HTTP、SSE、WebSocket、认证、共享和撤权逻辑。

两套实现的项目、会话、文件、Git、SSE、认证和 Host/Origin 处理存在重叠，但 Work 还额外需要：

- 一个多项目 `LocalWorkspace` owner；
- 本机访问与远程访问共用同一个 owner；
- 可撤销的共享授权；
- 请求 admission 和 owner shutdown 协调；
- Work 专属 WebSocket 实时协议；
- Work 组件、资源和专属面板。

目标不是把 Work 协议搬进 `rpi-package`，而是将以下通用机制沉淀到 `rpi-httpcore`：

```text
HTTP 认证
Host / Origin 校验
AuthLease
Sharing generation
授权撤销
请求 admission
SSE / WebSocket 连接生命周期
配对票据
shutdown / drain
```

Work 只保留业务协议和 owner 适配。

## 2. 目标边界

目标架构：

```text
rpi-extcore
    原生 Agent、Session、Resource、Provider 能力
        ↓
rpi-http-transport 或 rpi-httpcore::transport
    通用认证、Lease、撤权、连接生命周期和准入
        ↓
rpi-httpcore
    rpi Service 的标准 HTTP/SSE API

rpi-work
    work-node-runtime
        Work LocalWorkspace、多项目和业务状态
    work-node-service
        Work HTTP/WebSocket 协议适配器
    work-client / work-ui
        Work 客户端和界面
```

`rpi-package` 不得依赖：

- `work-node-api`；
- `work-node-runtime`；
- `work-client`；
- Dioxus 或 Work UI；
- Work 组件协议；
- Work 的项目注册表和本地数据格式。

通用 HTTP Core 必须通过抽象的 owner、操作执行器或连接上下文接入不同服务。

## 3. 必须提供的能力

### 3.1 AuthLease

提供可撤销的授权租约，而不是只返回一个永久有效的 Cookie。

要求：

- 每次成功认证生成一个不透明的 principal/session 标识；
- Lease 包含当前授权 generation；
- Lease 支持过期时间；
- Lease 不携带密码、原始凭据或项目路径；
- Lease 可用于 HTTP、SSE 和 WebSocket；
- Lease 校验必须是常数时间或不暴露敏感信息的安全比较；
- Lease 失效时返回统一的未授权/授权失效结果；
- 服务端不能仅依赖客户端主动 logout 来撤销租约。

建议的抽象信息：

```text
AuthLease
  principal
  generation
  expires_at
  connection/session metadata（可选）
```

具体字段和类型可以由 `rpi-package` 决定，但不能绑定 Work 类型。

### 3.2 Sharing generation

提供共享状态和单调递增的 generation。

要求：

- 共享状态至少包含 `enabled` 和 `generation`；
- 关闭共享时 generation 必须递增；
- 重新打开共享不能让旧 Lease 恢复有效；
- generation 递增必须原子化；
- 进程内所有 HTTP、SSE、WebSocket 连接看到同一份共享状态；
- generation 不能因为配置重新打开而回退；
- 本机私有控制通道可以使用独立的授权模式，但不能绕过 owner shutdown。

典型流程：

```text
共享开启，generation = 10
    ↓
客户端获得 Lease(generation = 10)
    ↓
关闭共享
    ↓
generation = 11
    ↓
所有 generation = 10 的 Lease 失效
    ↓
旧 SSE / WebSocket 连接停止发送数据
    ↓
新操作被拒绝
```

### 3.3 授权撤销与线性化

必须提供明确的授权检查点，避免“检查通过后撤权，但操作仍被当成有效操作执行”。

至少需要支持：

```text
authenticate
validate
authorize_before_commit
while_authorized
wait_invalidated
revoke
```

语义要求：

1. 请求进入时检查认证和共享状态；
2. 真正提交 owner 操作前再次确认 Lease 仍有效；
3. 撤权发生在提交前，操作不得开始；
4. 撤权发生在操作已经开始后，不声称能够回滚原生操作；
5. 操作结果发送给客户端前再次检查授权；
6. 如果结果发送前 Lease 已失效，不能把结果当作成功结果返回；
7. 未知、超时或进程断开不能被错误地当成安全可重试的拒绝。

这部分应成为公共契约，并有并发测试覆盖。

### 3.4 HTTP 认证和请求保护

现有 `rpi-httpcore` 已有密码、Cookie、Host/Origin 保护。迁移后仍需保留，并与 Lease 绑定。

要求：

- 密码只用于登录，不进入 URL、日志、错误消息或持久化状态；
- Cookie 至少具备 `HttpOnly`、`SameSite` 和正确的 `Secure` 行为；
- 支持明确的登录、状态查询和 logout；
- logout 必须撤销对应 Lease；
- 支持登录失败限流；
- Host allowlist 校验必须在路由执行前完成；
- Origin 校验必须覆盖普通 HTTP、SSE 和 WebSocket 握手；
- mutation 请求必须有明确的请求标记或等效防跨站机制；
- 错误响应不得回显密码、Cookie、内部路径、进程参数或 stderr；
- 认证和 extractor 错误也必须带安全响应头。

### 3.5 请求 admission

通用 HTTP Core 需要提供请求进入 owner 前的准入机制。

要求：

- 允许设置并发上限；
- 在获取 owner 操作资格前完成认证和共享检查；
- 队列满时返回明确的限流错误；
- 请求取消不能导致原生操作被错误重复执行；
- 已经开始的操作必须有明确的“结果未知”语义；
- admission permit 必须在请求完成、取消或失败时释放；
- shutdown 开始后禁止新的 owner 操作进入；
- 已进入执行阶段的操作按照 owner 的生命周期策略排空。

Work 当前的 `executor.rs` 和 `RealtimeState` 中有部分实现，应提取其通用语义，而不是直接复制代码。

### 3.6 SSE 生命周期

要求：

- SSE 建立时验证 AuthLease；
- 连接存活期间定期重新验证 Lease；
- logout、sharing revoke 或 shutdown 后停止发送事件；
- 支持 keepalive；
- 事件序号、重放游标和 resync 语义必须明确区分；
- 发生丢事件时必须发送 resync 信号，而不是伪造完整恢复；
- 连接断开必须释放订阅和资源；
- 不允许因为客户端重连而重复执行 owner 操作；
- 事件流不能绕过 owner 的授权和 shutdown 状态。

现有 `rpi-httpcore` 已经有普通 session SSE 和部分重放逻辑，Work 还有自己的实时协议。公共层应提供生命周期保护，事件 payload 和序列语义仍由具体协议拥有。

### 3.7 WebSocket 通用生命周期

`rpi-httpcore` 当前主要提供 HTTP/SSE；如果要承载 Work 的 WebSocket，需要补充通用 WebSocket 生命周期机制。

要求：

- WebSocket 握手阶段完成 Host/Origin 和 AuthLease 校验；
- 握手后的连接绑定到 Lease generation；
- Lease 失效时主动关闭连接；
- close code 和关闭原因必须稳定且不泄露内部信息；
- 支持 keepalive 和连接级 shutdown；
- 支持连接数和消息大小限制；
- 支持每连接和每节点的发送缓冲上限；
- 慢客户端不能无限占用 owner 或内存；
- 请求取消、连接断开和服务 shutdown 不能导致重复执行；
- 协议层可以注册自己的 frame decoder、encoder 和 operation executor。

通用 Core 只负责连接生命周期和安全，不能定义 Work 的 `RealtimeOperationKind` 或 Work 的业务 JSON frame。

### 3.8 shutdown 和 drain

HTTP Core 必须提供服务级排空能力。

要求：

```text
停止接受新连接
    ↓
拒绝新的 owner 操作
    ↓
通知 SSE / WebSocket 连接停止
    ↓
等待已进入执行阶段的请求
    ↓
释放订阅、permit 和 listener
    ↓
返回 drain 结果
```

同时要区分：

- HTTP listener 关闭；
- 网络连接排空；
- owner 操作排空；
- native child / Agent 退出。

HTTP Core 不应自行关闭 `rpi-extcore` 以外的 Work owner，但必须允许宿主注入 shutdown signal 和 drain callback。

### 3.9 一次性配对票据

QR 授权不是纯 Work 业务，可以把“票据机制”沉淀到通用 Core。

要求：

- 票据一次性消费；
- 有短 TTL；
- 与当前 sharing generation 绑定；
- 共享关闭后旧票据不可兑换；
- 票据不能作为长期 Bearer Token；
- 票据不能进入日志、URL 历史或错误回显；
- 兑换必须原子消费，防止并发重复兑换；
- 限制未完成票据数量；
- 支持 shutdown 时清理；
- 兑换后的权限由宿主决定，不能默认赋予全部能力。

二维码的展示格式、Work UI 和节点信息仍然由 Work 负责。

## 4. 路由和 owner 适配要求

### 4.1 不绑定单一 owner 类型

当前 `rpi-httpcore` 的 router 绑定：

```rust
Arc<rpi_extcore::service::Service>
```

通用传输层不能绑定这个具体类型。建议提供以下之一：

```text
通用 Access/Transport 类型 + 业务路由自己注入 owner
```

或：

```rust
trait HttpOwner {
    type Error;

    async fn execute(&self, request: OwnerRequest) -> Result<OwnerResponse, Self::Error>;
}
```

`rpi-httpcore` 可以用 `rpi_extcore::Service` 实现，Work 可以用 `LocalWorkspace` 实现。

### 4.2 路由组合

建议让通用 Core 支持：

```text
通用安全 middleware
通用 auth routes
通用 connection lifecycle
业务 router merge
```

例如概念上：

```rust
let app = http_core::protected_router(access_state)
    .merge(rpi_httpcore::rpi_routes(service))
    .merge(work_routes(local_workspace))
    .merge(work_realtime_routes(executor));
```

不要求最终 API 必须完全如此，但需要让不同 owner 组合路由，而不是复制整套 server 启动代码。

### 4.3 不允许依赖 Work

`rpi-package` 可以被 Work 依赖，但不能反向依赖 Work。

不能进入通用 Core 的内容：

- `LocalWorkspace`；
- `work-node-api`；
- Work 的 `/components` 业务；
- Work packages/native-resources；
- Work node ID 数据格式；
- Dioxus；
- Work 的本地 registry。

## 5. Work 需要保留的能力

下列内容不要求迁移到 `rpi-package`：

- Work 多项目 owner；
- Work 项目注册和路径映射；
- Work 组件包安装、面板和能力授权；
- Work 专属资源和 authoring；
- Work WebSocket frame 和 operation kind；
- Work 的 node identity；
- Work 的本机控制接口；
- Work 的客户端状态投影；
- Work UI 和 Web/Android 入口；
- Work-specific error mapping。

Work 只需要依赖通用的访问生命周期、连接生命周期和排空能力。

## 6. 标准 API 兼容要求

如果 Work 继续使用与 rpi HTTP 相同的 `/api/v1` 路径，则必须建立兼容性测试。

至少覆盖：

- auth status/login/logout；
- health/capabilities；
- projects 和 project folders；
- sessions 的创建、打开、重命名、归档、删除和 fork；
- snapshot、message summaries、message window；
- file/image/Git API；
- provider 和 auth refresh；
- ordinary SSE；
- tool-program read-only queries；
- 状态码和错误码；
- Host/Origin 和 mutation marker；
- Last-Event-ID、重放和 resync。

对于 Work 专属接口，应明确标注为 Work extension，而不是暗示 `rpi-httpcore` 原生支持。

## 7. 安全和并发验收标准

### 7.1 授权

- 旧 generation 的 Lease 在重新打开共享后仍然失效；
- logout 会让已有 SSE/WS 连接停止发送；
- 关闭共享后排队请求不能进入 owner；
- 撤权发生在响应发送前时不能发送成功结果；
- 操作开始后撤权不会伪造安全回滚结果；
- 旧 QR 票据不能在共享重新打开后兑换；
- 本机控制能力不能通过公开 HTTP 参数伪装获得。

### 7.2 并发

- 同一票据并发兑换最多成功一次；
- logout 与请求提交并发时结果符合线性化规则；
- sharing revoke 与 SSE/WS 写入并发时不发送撤权后的数据；
- shutdown 与新请求并发时不会启动新 owner 操作；
- 慢 WebSocket 客户端不会无限增长缓冲；
- 请求取消不会造成第二次执行。

### 7.3 信息泄露

- 错误不回显密码、Cookie、票据、路径、命令行参数和 stderr；
- 日志不包含认证凭据；
- 响应不会把本地路径和 native 进程信息暴露给远程客户端；
- 公开 HTTP 不会因为安装了 Work UI 组件而自动获得本机能力。

## 8. 推荐实施顺序

### 阶段一：抽象契约

在 `rpi-package` 中先定义通用概念和测试：

```text
AuthLease
AccessState / SharingState
Generation
Revocation
Admission
Drain
Pairing ticket
```

暂时不改 Work 路由。

### 阶段二：迁移 Work 的安全机制

让 Work 的 `work-http-runtime` 使用 `rpi-package` 的通用能力：

- 保留 Work 的配置和 owner 绑定；
- 删除重复的 generation、Lease 和 revoke 实现；
- 保留 Work 特有的 pairing 权限映射；
- 保留 Work 的错误和 UI 行为。

### 阶段三：迁移实时生命周期

将下面这些公共逻辑迁移到通用层：

- SSE 授权重检；
- WebSocket 授权失效关闭；
- admission permit；
- shutdown drain；
- backpressure 和消息上限。

Work 只提供事件和操作 payload。

### 阶段四：标准 API 对齐

建立 `rpi-httpcore` 与 `work-node-service` 的兼容性矩阵，确认：

- 哪些路径保持一致；
- 哪些接口是 Work extension；
- 哪些接口只属于 rpi-httpcore；
- 哪些错误和事件格式需要统一。

### 阶段五：清理 Work 重复代码

完成迁移后再决定：

- 删除 `work-http-runtime`；
- 将其改名为 `work-node-access`；
- 或保留为很薄的 Work owner 配置层。

不要在通用抽象稳定前直接删除现有 HTTP 实现。

## 9. 不属于本需求的内容

本文不要求 `rpi-package`：

- 实现 Work 的 `LocalWorkspace`；
- 实现 Work 的多项目 registry；
- 接入 Work UI；
- 迁移 Work component bundle；
- 直接实现 Work 的 `/api/v1/components`；
- 直接实现 Work 的 native resources/authoring；
- 代替 Work 的 `work-client`；
- 修改 rpi 原生 Agent、ACP 或资源协议；
- 让 `rpi-httpcore` 依赖 Work。

## 10. 最终判断

需要沉淀到 `rpi-package` 的是通用机制：

```text
可撤销授权租约
共享 generation
授权线性化
请求 admission
SSE/WS 生命周期
连接排空
一次性配对票据
通用安全 middleware
```

不需要沉淀到 `rpi-package` 的是 Work 业务：

```text
Work owner
Work 多项目模型
Work HTTP 路由语义
Work WebSocket frame
Work components
Work resources
Work UI
```

最终目标是：

```text
rpi-httpcore 负责通用 HTTP/实时基础设施和 rpi API
Work 负责自己的 owner 和业务协议
两者共享安全、授权、撤权和连接生命周期机制
```
