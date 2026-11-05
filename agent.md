# RPI Work 本地开发与优化启动

## 一键启动最优 Web 版本

在仓库根目录执行：

```bash
task dev
```

`task dev` 现在是 release/headless 路径，会依次完成：

1. 构建 `work-ui` 的 release WASM；
2. 为 Web 产物生成内容哈希目录，例如 `releases/work-ui-<hash>/`；
3. 为版本化 JS/WASM 启用长期 immutable 缓存；
4. 为入口 HTML 和 `build.json` 保留重新校验；
5. 通过 Brotli/gzip 压缩 HTTP 响应；
6. 构建 release `work-cli`；
7. 启动无头 owner，并由同一个 `work-server` 直接托管 Web 资源。

默认访问地址：

- 本机：`http://127.0.0.1:13031/`
- 局域网手机：`http://<电脑局域网IP>:13031/`
- EasyTier：使用本机 EasyTier 地址的 `13031` 端口

`.env.local` 必须配置非空的 `RPI_WORK_HTTP_PASSWORD`，也兼容 `RPI_HTTP_PASSWORD`。密码不会写入命令行参数、日志或构建产物。

可选配置：

```dotenv
RPI_WORK_HTTP_BIND=0.0.0.0
RPI_WORK_HTTP_PORT=13031
RPI_WORK_HTTP_PUBLIC_URL=http://<手机可达的电脑地址>:13031/
RPI_WORK_HTTP_ASSETS=target/parity-web
```

`task dev` 前台运行，使用 `Ctrl+C` 停止 owner。不要在同一个数据目录下额外启动 `task desktop`、另一个 `work-cli serve` 或 `work-ui --headless`，否则会触发 `node.lock` 冲突。

## 缓存策略

- `index.html`、`build.json`：`no-cache, must-revalidate`，用于发现新版本；
- `releases/work-ui-<hash>/` 下的 JS/WASM/CSS：`public, max-age=31536000, immutable`；
- 根目录兼容资源：短 TTL，避免固定文件名长期保存旧版本；
- Web 响应根据客户端能力使用 Brotli 或 gzip；SSE 不压缩，避免影响实时事件刷新。

缓存只减少重复打开时的下载；首次打开仍需下载 WASM。release 构建比 debug WASM 小很多，版本化资源可以让浏览器安全长期缓存，不会因为旧缓存阻止新版本加载。

## 其他启动路径

- `task desktop`：桌面窗口开发路径；
- `task dev:desktop`：桌面窗口 + `14180` Web 热刷新调试路径，使用 debug WASM，适合频繁改 UI；其产物位于 `target/parity-web-debug/parity-web`，不会覆盖 `task dev` 的 release 资源；
- `task node`：显式独立 standalone 节点，默认 `13030`，不是桌面/无头 owner。

## 验证优化

```bash
curl -I http://127.0.0.1:13031/
curl -I http://127.0.0.1:13031/build.json
# 从 index.html 找到 releases/work-ui-<hash>/work-ui_bg.wasm 后检查：
curl -I http://127.0.0.1:13031/releases/work-ui-<hash>/work-ui_bg.wasm
```

预期结果：

- 入口返回 `Cache-Control: no-cache, must-revalidate`；
- 版本化 WASM 返回 `Cache-Control: public, max-age=31536000, immutable`；
- 支持压缩时响应包含 `Content-Encoding: br` 或 `Content-Encoding: gzip`；
- `Content-Type` 为 `application/wasm`。

## 安全边界

当前默认是局域网 HTTP，仅适合可信网络。不要直接把 `13031` 暴露到公网；公网访问应使用 HTTPS 反向代理，并保持 Host、Origin 和节点密码校验。
