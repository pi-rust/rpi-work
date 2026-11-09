# 精选 Package 展示、下载与节点获取

这是正式资源页的**资源型 Package / Skill 分发入口**，不是旧的惰性设计包预览，也不是在线包市场。安装和会话加载继续使用[现有原生资源链路](native-resource-authoring.md)。

## 使用流程

1. 连接并认证目标执行节点，选择一个已授权项目，打开 **Packages / 技能库 → 执行资源 → 随应用发布的精选目录**。
2. 按名称、描述、作者或类型筛选，打开详情，核对版本、来源、SHA-256、指令映射和只读文件内容。
3. 根据目标选择：
   - **下载到本机**：通过浏览器 / WebView 请求保存 UTF-8 JSON 分发信封。不获取到执行节点，不安装；以实际下载列表和保存结果为准。
   - **获取到当前节点**：节点把内嵌的固定资源内容准备到节点自有来源目录，校验后回读目录。浏览器无需提供节点文件路径。
4. 获取回读确认后，选择 **当前项目 / 节点全局**，检查内容并明确勾选信任，点击 **安装**。原生管理器的回执失败或不支持时，界面不会显示安装成功，也不会改走另一节点。
5. 在下方原生目录刷新核对真实安装事实。使用已有会话控件显式选择、填写需要的 Package 指令映射并应用，再核对会话实际加载、模型有效工具与执行就绪状态。安装不自动修改、运行或重启任何会话。

切换包、安装范围、项目或连接上下文后需要重新确认信任。离页、断线或超时不等于节点取消写入；请先刷新权威状态，不自动重试安装。

## 首版目录与边界

| 资源 | 版本 | 内容来源 |
| --- | --- | --- |
| `rpi-work-base` | `1.0.0` | `assets/packages/base`，与普通新会话的内置基础包相同 |
| `rpi-work-developer` | `1.0.0` | `assets/packages/developer`，与开发会话的内置包相同 |
| `work-basics` | `1.0.0` | 基础包中的同一 `SKILL.md`，独立 Skill 安装形式 |

- 目录随应用编译发布，无线上 registry、后台刷新、任意 URL、Git clone、压缩包解压或构建脚本执行。
- 只接受代码内受控的固定身份 / 版本 / 摘要引用；不提供用户自定义下载地址或通用导入 API。
- 首版只分发 Markdown 文本。没有 DLL、动态库、脚本、原生扩展平台/ABI 兼容承诺或自动升级。
- `PackageBundle` JSON 是包含元数据、排序源码和摘要的**传输信封**，不是上游 rpi 可执行安装清单。当前原生安装使用信封对应的 canonical 源码目录，不能将下载的 JSON 文件路径直接填作安装来源。
- 导出文件可用于保存、审阅和比对，但本批未提供通用跨节点导入或将任意本地发布物上传到目录的能力。不要把旧的 `rpi-work-design-package` 导入入口当作它的安装器。
- 本机下载能力取决于浏览器 / WebView 宿主，JS 下载请求成功不证明文件已经落盘。当前行为验收覆盖 Chromium，不替代 Windows WebView2 和 Android 下载验收。

## 状态与身份

**已获取 ≠ 已安装 ≠ 会话已选择 ≠ 实际加载 ≠ 对模型有效。**

精选目录中的 `acquired` 仅表示：本次观察时，这个执行节点的受控来源目录具有完整且精确匹配的文件树。它不是客户端缓存的安装布尔值。缓存属于节点，同一节点的其他授权项目也可能显示已获取；项目安装范围和会话配置仍相互独立。

基础与开发包复用已有 `<node-data>/shipped/1.0.0/<name>` 来源身份，不另复制一套会话资源或创建第二个 owner。独立 Skill 也位于节点自有 shipped 根中。普通会话先前准备过相同字节时，可以直接观察为已获取，但仍需以原生目录判断安装。

分发 SHA-256 是 `serde_json` 紧凑序列化 `(manifest, sources)` 的摘要；源码映射按路径排序，元数据也参与摘要。它与原生安装器自己的文件树 `recordedDigest` 不是同一种摘要，不能直接互比。校验和用于内容身份，不等于来源签名、代码沙箱或权限授予。

同名同版本出现不同或额外字节会拒绝获取和安装，保留原内容，目录给出诊断；不会自动覆盖、删除或降级到新来源。缺少但仍匹配的部分文件可由显式获取补全。发布新内容时应更新版本，不将同版本不同字节视为自动更新。

## 安全与生命周期

- typed `NodeApi`：`package_catalog`、`package_bundle`、`package_acquire`、`package_install`。本机门面和 HTTP 访问同一节点 owner，旧实现默认 Unsupported。
- HTTP 适配仍受原认证、精确 Host / Origin、写请求标记、共享和 admission 边界保护：
  - `GET /api/v1/projects/{id}/packages`
  - `POST /api/v1/projects/{id}/packages/bundle`，正文为 `PackageRef`
  - `POST /api/v1/projects/{id}/packages/acquire`，正文为 `PackageRef`
  - `POST /api/v1/projects/{id}/packages/install`，正文为 `PackageInstall`
- 请求版本 / 摘要必须精确匹配当前内嵌目录。未知字段、来源 URL 或猜测的新版本不会进入节点路径解析。
- 源码上限 64 文件、总计 1 MiB、单文件 256 KiB、路径 240 字节和 12 层；拒绝路径穿越、隐藏路径、Windows 设备 / ADS / 尾点别名、大小写冲突、文件 / 目录冲突、额外文件、链接 / junction / reparse 和特殊文件。Unix 字面反斜杠不能冒充目录分隔符。
- 文件 IO 在保留的 blocking worker 中执行，持有 owner、串行获取门和生命周期读租约直到实际退出。客户端取消不丢弃这些边界，节点协作退出等待 worker；不声称能对任意设备 IO 卡顿、强杀或断电提供相同保证。
- 安装再次核对已获取内容和明确授权，再委托已有 `native_manage`。原生管理器仍负责事务、在用租约和回执；分发层不新增安装引擎或会话配置事实。
- 这些目录检查不是 OS 沙箱，不能保护针对恶意、不合作外部文件写入者的完整 TOCTOU 原子性；沿用原生资源指南的 filesystem 限制。

## 验证与复现

```powershell
cargo test -p work-node-runtime --lib workspace::packages --locked --offline
cargo test -p work-node-service --test package_distribution --locked --offline
cargo test -p work-ui --bin work-ui features::resources::packages --locked --offline
node --test tests/structure/curated-packages.test.cjs
cargo build -p work-cli -p work-node-service --bin work-cli --bin work-server-rpc-fixture --locked --offline
& tools/dev/build-ui-web.ps1 -DebugBuild -OutputRoot target/curated-package-build
node tests/e2e/verify-curated-packages.cjs
```

浏览器脚本校验正式 WASM 的 source hashes，启动随机回环端口、临时 home / data 的真实无头 owner，仅使用仓库 synthetic RPC fixture，不调用真实模型。覆盖目录 / 筛选 / 预览、真实 JSON 下载及摘要、节点获取回读、安装信任与范围、Unsupported 安装不伪成功、刷新 / 重载 / 项目切换、实际明暗主题与 1440 / 390 / 320 布局。报告和截图生成在 `artifacts/curated-packages/`；失败历史报告不代表最新结果。

Linux 额外验证使用现有 WSL 与私有离线 Cargo 缓存副本，不下载依赖、不改共享缓存或原 workspace / lock：

```sh
MSYS_NO_PATHCONV=1 wsl.exe --distribution Ubuntu24 --exec \
  python3 /mnt/d/Projects/rpi-work/tests/support/unix-package-distribution.py
```

本批没有重新验收成功的真实原生安装 → 会话加载 → 模型执行。安装复用链路的既有真实隔离验收和显式 opt-in 复现见[原生资源指南](native-resource-authoring.md#reproduction-and-evidence)，不得用本批 fixture 拒绝测试代替成功验收。没有完成 Windows 交互桌面和 Android 真机下载测试；Android client / mobile UI 编译检查也不是设备行为证明。严格 Clippy 仍受现有基线告警阻断，诊断性 lint 降级运行不算严格通过。
