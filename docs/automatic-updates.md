# 应用更新

设置 → 应用更新：默认启动时及每六小时检查GitHub最新稳定Release；可以关闭自动检查或手动检查。仅比较SemVer更高的版本，不自动降级。首次联网失败不会阻止使用会话，手动检查可重试。

发现更新后出现可关闭的提醒。下载由用户启动，显示进度；桌面端通过“安装并重启”应用更新，Android通过“安装更新”打开系统确认。会话正在发送、运行或保留未发送草稿/附件时禁用安装，避免重启丢失输入；离开更新页仍会继续检查与下载。电脑节点进程不在更新包中。

## 验证与安装

- 原生客户端从固定仓库 `pi-rust/rpi-work` 的最新稳定Release下载 `update.json` 和 `update.json.sig`。清单使用Ed25519签名，公钥编译进客户端；不接受未签名、错误签名或标签版本不一致的清单。
- 原生更新包按OS和CPU选择，校验签名清单中的文件名、大小与SHA-256；限制256MiB。下载失败清理本次临时目录，不覆盖当前安装。
- 桌面只解压指定UI和CLI文件，拒绝越界路径和链接。安装前再次校验暂存文件；复制当前可信程序作为独立助手，等待原程序退出，备份现有程序后替换，失败尝试回滚并重新打开旧版本。需要安装目录写权限；失败日志保留在更新缓存中的 `install-result.txt`。便携安装请保留原程序名 `work-ui` / `work-ui.exe`。
- Android使用应用私有缓存，仅通过只读ContentProvider临时授权系统安装器访问已验证APK。安装前校验包名、递增versionCode和当前签名证书；首次允许应用安装软件后需要返回再点击安装。用户始终在系统界面确认。JNI入口使用Keep注解，避免release混淆后失效。
- Web通过支持CORS的GitHub API检查版本。Web静态资源由电脑服务端部署，浏览器不会下载并执行原生更新包；部署后用户重新加载页面。运行中的会话阻止更新页重新加载。

## 发布流程

普通main推送与PR仅编译、上传Actions制品，不对用户推送开发构建。版本发布时：

1. 更新 `apps/work-ui/Cargo.toml` 的版本及 `docs/release-notes.md`，运行验证并提交。
2. 创建与UI版本一致的 `v<version>` 标签并推送。
3. `build.yml` 在检查与八个平台构建全部成功后收集制品、校验SHA-256并生成清单。
4. 发布任务验证私钥与仓库中的公钥匹配后签名，创建draft Release，上传完整制品与签名清单，最后公开Release并标记latest。

签名缺失、版本不匹配、平台制品不完整均会阻止发布。已公开的同版本Release不会被脚本覆盖；更新必须使用新版本。不包含iOS、应用商店签名或桌面OS公证。

仓库Secrets：`UPDATE_SIGNING_KEY`（Ed25519 PEM）、`ANDROID_KEYSTORE_BASE64`（PKCS12）、`ANDROID_KEYSTORE_PASSWORD`。私钥不进入Git。当前签名备份在本机用户目录 `.config/rpi-work/signing/`，已限制为当前Windows用户访问；需另行安全备份，尤其不要丢失Android签名证书。公钥在 `crates/work-update/update-public-key.txt`；更换更新签名密钥需规划旧版本信任迁移，不能直接替换后指望旧客户端接受。

Android release包使用固定证书；缺少密钥的本地/PR构建生成带 `-dev` 的开发包，不进入更新清单。Dioxus 0.7.10模板的versionCode恒为1，脚本重新构建Gradle release，设为 `major×1,000,000 + minor×1,000 + patch`，minor和patch要求小于1000。Android 0.1.0原临时开发签名包无法覆盖升级为固定签名版，需先卸载旧包；手机本地偏好会清除，电脑节点数据保留。

## 本机验证

运行 `cargo test -p work-update` 与 `scripts/verify-rust-workspace.ps1`，再使用 `scripts/build-ui-web.ps1` 和 `scripts/verify-updates.cjs` 验证设置、版本发现、失败重试、偏好保存和窄屏布局。安装助手测试只操作隔离临时目录；不替换当前工作区或用户已安装程序。Android构建使用 `scripts/ci/build-android.ps1`，模拟器验证不代表真机或应用商店审核结果。
