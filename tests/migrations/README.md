# 空闲会话的受控复制迁移

`tools/dev/migrate-idle-sessions.py` 将本机 RPC 的空闲会话复制到一个**已离线**的 HTTP 节点。它不会停止或发送信号给任何进程，不更改源登记或 JSONL；不读取密码。当前会话必须明确排除，其他锁定会话也会跳过。

```powershell
python -m unittest discover -s tests/migrations -v
python tools/dev/migrate-idle-sessions.py `
  --source-data "$env:LOCALAPPDATA/rpi-work/node" `
  --http-data "$env:USERPROFILE/.rpi-web" `
  --default-project 'D:/Projects/rpi-work' `
  --exclude-session '<当前正在使用的会话 ID>'
# 核对预检后，同一命令追加 --apply。不要照抄占位 ID。
```

默认只预检（会暂时获取对应 `.jsonl.lock` 文件的锁，必要时创建锁文件），`--apply` 才复制历史、备份和替换目标登记。所有复制历史都有 SHA256 校验；归档、置顶、标签、标题、时间、分叉关系与目标未知字段保留。目标已有相同 ID 时拒绝再次导入，不覆盖、不生成重复条目。

输出位于 HTTP 数据目录：
- `migrations/<时间戳>/`：目标旧登记、源项目/会话登记、选中历史和 timing 备份、合并后的登记、迁移 journal。
- `session-store-<时间戳>/`：HTTP 的独立历史目录，包含迁入历史和可定位的既有 HTTP 历史；不与源共用写入文件。
- `node-migrated-<时间戳>.json`：仅配置路径、项目、回环地址及端口，不含凭据。源项目通过显式 `projects` 列表注册，不扩大 managed root。

迁移后需要让 HTTP 使用新配置：在本机 Git ignored `.env.local` 设置 `RPI_HTTP_CONFIG` 为输出的配置绝对路径，`task node` / `task dev` 会读取它；认证仍由原有环境变量提供。直接运行二进制时使用 `rpi-http --config <配置路径>`。不要忽略配置重启，否则会回到旧的历史目录。**节点已运行时不要重复启动这些命令。**

这是一次性复制，不是两个节点持续同步。源数据保持完整；迁入会话之后应在 HTTP 节点接续，旧本机上的同 ID 副本保持为旧分支，不会自动合并后续消息。脚本不会自动退役源节点、改客户端当前连接或迁入锁定会话。目标原先就无法定位的历史会列入 `preexistingUnresolvedHistories`，保留登记而不伪造空历史，需另行处理。

验证报告应区分：文件逐字节校验、认证后目录校验，以及通过 HTTP `open` + 有限 `snapshot` 的历史恢复。恢复读取会启动该 HTTP 节点管理的原生宿主，但不得发 prompt、resume 或模型配置命令。

若需回滚，必须先由使用者安排目标节点离线，再恢复备份中的 `target-sessions.json`、原启动配置及必要的 `target-projects.json`；不得在线覆盖 registry。保留 `session-store`，因为迁移后产生的新消息可能只存在其中。未验证成功的 journal 不等于已完成迁移；不要自动重试 `--apply`。
