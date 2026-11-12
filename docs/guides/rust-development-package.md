# Rust 开发环境 Package 快照

## 范围

`tools/dev/package-rust-environment.py` 从本机已安装的 Rust Skills 和明确列出的开发扩展创建独立目录快照，不修改原始全局资源。输出目录必须不存在。

```bash
python -B tools/dev/package-rust-environment.py --output target/rust-development-export-v4
```

输出：

- `rust-development/`：**原生安装来源目录**；包含 Skills、Windows 原生扩展、Package 系统指令、环境清单与内容 inventory。
- `selection.json`：项目级 `main_package` 精确选择及默认工具选择；无需启用全局资源。
- `checksums.json`：Package 每个文件的 SHA256 和大小。
- `rust-development-1.0.0-windows-x64.zip`、`archive.json`：传输文件及其校验信息，**不是**原生安装入口。

当前快照包含 18 个 DLL、27 个 Skills、72 个文件，共 32,229,968 字节。Dioxus 不依赖全局 registry 是否及时刷新：会补充已安装的 Dioxus，并使用其 frontmatter 声明的 canonical 名称 `dioxus-knowledge-patch` 建立目录。目录名与 Skill 元数据名不符会使原生宿主拒绝加载。

不包含 auth、models、全局 settings、会话历史、Rust 编译器、SDK、Cargo 缓存或外部 codegraph/MCP 服务。`environment.json` 与 `rust-toolchain.toml` 只是环境描述，不会替目标机器安装工具链。DLL 是本机 Windows x64 原生产物，不能直接用于 Linux/macOS，也不保证兼容任意 RPI ABI；对外分发前还需核对资源许可。

## S3 下载（RustFS）

- [Windows x64 ZIP](https://origin-assets.bigfisharc.cn/rpi-package/releases/rust-development-v1.0.0/rust-development-1.0.0-windows-x64.zip)
- [ZIP SHA256](https://origin-assets.bigfisharc.cn/rpi-package/releases/rust-development-v1.0.0/rust-development-1.0.0-windows-x64.zip.sha256)
- [逐文件校验清单](https://origin-assets.bigfisharc.cn/rpi-package/releases/rust-development-v1.0.0/checksums.json)
- [原生会话选择配置](https://origin-assets.bigfisharc.cn/rpi-package/releases/rust-development-v1.0.0/selection.json)

S3 对象：`s3://assets/rpi-package/releases/rust-development-v1.0.0/rust-development-1.0.0-windows-x64.zip`。
下载域名直接反代既有 RustFS S3 API；沿用 `assets` 原有公开读取策略，没有变更 bucket 权限。

ZIP 为 14,405,693 字节，SHA256：

```text
8b27cd22d36ceb10071ce071aafe504d3a12d6382b9039e945ab224557ab8291
```

## 原生安装与会话选择

在**目标项目目录**执行，使用经过独立核对的兼容原生 RPI：

```bash
rpi resource install package rust-development --scope project --path /absolute/export/rust-development --version 1.0.0
rpi resource inspect package rust-development --scope project
rpi --resource-config /absolute/export/selection.json
```

安装是将 canonical 源码目录交给原生资源管理器；不要将 ZIP、`archive.json`、`inventory.json` 或 `PackageBundle` JSON 传给 `resource install`。接收 ZIP 时先在目标机器解压并核对 `archive.json` 和 `checksums.json`。

这些示例会创建目标项目的 `.rpi/packages`；不要为了验收而在真实共享项目中直接执行。自动验收为此创建独立项目、agent 配置、Cargo fixture 和会话目录。

## 自动验证

不启动任何原生进程的导出器回归测试：

```bash
python -B -m unittest discover -s tests/unit -p test_rust_environment_package.py -v
```

原生 CLI/RPC 验证（必须独立确认可信二进制摘要，不要自动计算当前文件摘要后当作信任来源）：

```bash
python -B tests/e2e/rust-development-package.py \
  --native /absolute/trusted/rpi.exe \
  --native-sha256 TRUSTED_SHA256 \
  --export target/rust-development-export-v4 \
  --output target/rust-development-smoke-new
```

脚本在任何原生启动前核对摘要，输出目录也必须不存在。验证分层进行：

1. 从目录安装 Package，核对复制后的文件摘要与原生安装/inspect 回执。
2. 使用独立 agent 配置和明确 `--resource-config` 启动独立原生 RPC 会话；不启动 work-node-service owner。
3. 读取 `get_resource_state`，确认 Package、全部 18 个 DLL、全部 27 个 Skills、无 Skill diagnostics 以及 33 个 model-effective tools。
4. 让原生宿主连接本机 loopback SSE 模型 fixture，检查实际模型请求中的 Package 指令及有效工具 schema。
5. 由 fixture 返回真实工具调用，执行 `read`、`code_lens` 的 Cargo check、`codemode` catalog；检查具体结果和最终 assistant 消息。
6. 退出宿主后重新打开同一个隔离会话，核对新的 hostId 与不变的会话/资源绑定；最后关闭所有测试进程与 HTTP fixture。

`result.json`、`install.json`、`inspect.json`、`loaded.json`、`reopened.json`、RPC events 和模型请求保存在测试输出目录。fixture 只使用本地占位凭据，不调用远端模型。

## 已验证与未验证边界

本机 CLI/RPC 冒烟验证使用的独立已安装二进制 SHA256 为：

```text
57620789e5d49e9c65b369ac9a8d6063c88b703a9049322832f5c4dd3317b718
```

这是**独立 CLI/RPC 层的验证**，不替代 `tests/e2e/native-resource-workflow.py` 或 `crates/transport/work-node-service/examples/resource_workflow.rs` 的集成验收。后两者要求的 pin 仍是：

```text
77a9f89d61aadab02f9799ac6cd69fb194258f7369f195d553e8733f29ded4dc
```

尚未找到该摘要对应的可信产物，故没有修改该 pin，也没有把其他二进制用于这两个集成验收入口。work-node-service/UI 资源工作流仍需单独验收。

全量 DLL 加载、工具 schema 可见、少量工具成功执行是不同证据。本次没有验证所有扩展行为、远端 provider、codegraph CLI、外部 MCP、移动 SDK 或跨机/跨平台兼容性；已有 work-ui 未提交修改也不在此次变更范围。
