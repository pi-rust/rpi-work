import { useEffect, useState } from 'react'
import QRCode from 'qrcode'
import { QrCode } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { SettingsCategory } from '@/components/settings-dialog'
import type { NodeProvider } from '@/lib/node'

type Props = {
  active: SettingsCategory
  live: boolean
  origin: string | null
  project: string
  session: string
  providers: NodeProvider[]
  revision: string | null
  error: string | null
  onRefresh: () => void
  authorization: { payload: string; expiresAt: number } | null
  authorizationError: string | null
  onIssueAuthorization: () => void
  onSaveKey: (id: string, key: string) => Promise<boolean>
  notice: string | null
  nodeInfo: { version: string | null; service: string | null; protocol: string | null; nodeId: string | null } | null
}

/** Existing node management bindings, presented inside the settings dialog. */
export function SettingsPage({
  active,
  live,
  origin,
  project,
  session,
  providers,
  revision,
  error,
  onRefresh,
  authorization,
  authorizationError,
  onIssueAuthorization,
  onSaveKey,
  notice,
  nodeInfo,
}: Props) {
  return (
    <div className="flex min-w-0 flex-col gap-6">
      <Section hidden={active !== 'connections'} title="节点与协作" subtitle="同一台电脑的桌面窗口与浏览器共享这个节点。">
        {live ? (
          <div className="flex flex-col gap-2 text-[12px]">
            <Row label="节点地址" value={origin ?? '—'} mono />
            <Row label="当前项目" value={project || '—'} mono />
            <Row label="当前会话" value={session || '—'} mono />
            <div className="flex flex-col gap-2 pt-1">
              <Button
                variant="outline"
                size="sm"
                className="h-7 w-fit gap-2 text-[12px]"
                onClick={onIssueAuthorization}
              >
                <QrCode className="size-3.5" />
                生成一次性授权码
              </Button>
              {authorizationError ? (
                <p className="text-destructive text-[11px]">{authorizationError}</p>
              ) : null}
              {authorization ? <AuthorizationQr payload={authorization.payload} expiresAt={authorization.expiresAt} /> : null}
            </div>
          </div>
        ) : (
          <p className="text-muted-foreground text-[12px]">当前没有连接真实节点；连接后才能读取节点信息。</p>
        )}
      </Section>

      <Section
        hidden={active !== 'providers'}
        title="提供商"
        subtitle="读取当前节点目录；保存密钥返回管理回执，不代表当前 Agent 已加载。"
        action={
          <Button
            variant="outline"
            size="sm"
            className="h-7 text-[12px]"
            onClick={onRefresh}
            disabled={!live}
            data-testid="refresh-providers"
          >
            刷新
          </Button>
        }
      >
        {error ? <p className="text-destructive text-[12px]">{error}</p> : null}
        {notice ? <p className="text-[12px] text-emerald-600">{notice}</p> : null}
        {!live ? (
          <p className="text-muted-foreground text-[12px]">连接节点后读取提供商目录。</p>
        ) : providers.length === 0 && !error ? (
          <p className="text-muted-foreground text-[12px]">{revision === null ? '尚未取得提供商目录。' : '节点没有返回任何提供商。'}</p>
        ) : (
          <div className="overflow-hidden rounded-lg border">
            <div className="bg-muted/40 text-muted-foreground grid grid-cols-[minmax(0,1fr)_48px_60px] gap-2 px-3 py-1.5 text-[11px] md:grid-cols-[minmax(0,1fr)_90px_90px]">
              <span>提供商</span>
              <span>模型</span>
              <span>状态</span>
            </div>
            {providers.map((provider) => {
              const models = provider.definition?.models?.length ?? 0
              const ready = provider.configured || provider.configKeyPresent
              return (
                <div
                  key={provider.id}
                  className="hover:bg-muted/30 grid grid-cols-[minmax(0,1fr)_48px_60px] items-center gap-2 border-t px-3 py-2 text-[12px] md:grid-cols-[minmax(0,1fr)_90px_90px]"
                  title={provider.diagnostic ?? provider.id}
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="truncate font-mono">{provider.id}</span>
                    {provider.storedCredential ? (
                      <Badge variant="outline" className="h-4 px-1 text-[9px]">
                        {provider.storedCredential}
                      </Badge>
                    ) : null}
                  </span>
                  <span className="text-muted-foreground text-[11px] tabular-nums">{models || '—'}</span>
                  <span className="flex items-center gap-2">
                    <Badge
                      variant={ready ? 'secondary' : 'outline'}
                      className={cn('h-5 w-fit px-1.5 text-[10px] font-normal', ready && 'text-emerald-600')}
                    >
                      {ready ? '已配置' : '缺失'}
                    </Badge>
                  </span>
                  <div className="col-span-full"><ProviderKeyForm id={provider.id} onSave={onSaveKey} /></div>
                </div>
              )
            })}
          </div>
        )}
        {revision ? (
          <p className="text-muted-foreground mt-2 font-mono text-[10px]">revision {revision.slice(0, 12)}…</p>
        ) : null}
      </Section>

      <Section hidden={active !== 'about'} title="应用更新" subtitle="自动更新需要签名清单与平台安装器，属于桌面端能力。">
        <ul className="text-muted-foreground flex flex-col gap-1 text-[12px]">
          <li>· Web 入口不支持自更新：请升级运行该节点的桌面端或 CLI。</li>
          <li>· 节点版本：{nodeInfo?.version ?? '—'}{nodeInfo?.service ? `（${nodeInfo.service}）` : ''}</li>
          <li>· 协议：{nodeInfo?.protocol ?? '—'}</li>
          <li>· 节点标识：<code className="font-mono text-[11px]">{nodeInfo?.nodeId?.slice(0, 16) ?? '—'}…</code></li>
        </ul>
      </Section>

      <Section hidden={active !== 'about'} title="关于" subtitle="ReUI 工作台。">
        <ul className="text-muted-foreground flex flex-col gap-1 text-[12px]">
          <li>· 展示规则、协议与主题来自 Rust（`work-viewmodel` / `work-client` 的 wasm 绑定）</li>
          <li>· 组件来自 ReUI（MIT）与 shadcn/ui（MIT）</li>
          <li>· 自动更新为桌面端能力，Web 原型不提供</li>
        </ul>
      </Section>
    </div>
  )
}

/** Renders the sealed envelope the phone scans; the QR encoder is the only
 *  browser-side dependency, the payload itself comes from Rust. */
function AuthorizationQr({ payload, expiresAt }: { payload: string; expiresAt: number }) {
  const [image, setImage] = useState<string | null>(null)
  useEffect(() => {
    let cancelled = false
    void QRCode.toDataURL(payload, { margin: 1, width: 168 }).then((url) => {
      if (!cancelled) setImage(url)
    })
    return () => {
      cancelled = true
    }
  }, [payload])
  const minutes = Math.max(0, Math.round((expiresAt - Date.now()) / 60000))
  return (
    <div className="flex items-start gap-3">
      {image ? (
        <img src={image} alt="一次性授权二维码" className="border-line size-[168px] rounded-lg border" />
      ) : (
        <div className="border-line size-[168px] animate-pulse rounded-lg border" />
      )}
      <div className="flex min-w-0 flex-col gap-1 text-[11px]">
        <span className="text-muted-foreground">用手机扫描；{minutes} 分钟内有效。</span>
        <code className="text-muted-foreground max-h-24 overflow-y-auto break-all font-mono text-[10px]">
          {payload.slice(0, 160)}…
        </code>
      </div>
    </div>
  )
}

/** Inline key entry: memory-only input, cleared as soon as the write returns. */
function ProviderKeyForm({ id, onSave }: { id: string; onSave: (id: string, key: string) => Promise<boolean> }) {
  const [open, setOpen] = useState(false)
  const [value, setValue] = useState('')
  const [busy, setBusy] = useState(false)
  if (!open) {
    return (
      <Button
        variant="ghost"
        size="sm"
        className="h-5 px-1 text-[10px]"
        aria-label={`设置 ${id} 的密钥`}
        onClick={() => setOpen(true)}
      >
        设置密钥
      </Button>
    )
  }
  return (
    <span className="flex min-w-0 flex-wrap items-center gap-1">
      <Input
        type="password"
        autoComplete="off"
        value={value}
        onChange={(event) => setValue(event.target.value)}
        aria-label={`${id} 的 API 密钥`}
        className="h-6 min-w-0 flex-1 text-[12px]"
      />
      <Button
        size="sm"
        className="h-6 px-2 text-[10px]"
        disabled={busy || value.trim().length === 0}
        aria-label={`保存 ${id} 的密钥`}
        onClick={async () => {
          setBusy(true)
          const ok = await onSave(id, value)
          setBusy(false)
          if (ok) {
            setValue('')
            setOpen(false)
          }
        }}
      >
        {busy ? '保存中…' : '保存'}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1 text-[10px]"
        aria-label="取消"
        onClick={() => {
          setValue('')
          setOpen(false)
        }}
      >
        取消
      </Button>
    </span>
  )
}

function Section({
  title,
  subtitle,
  action,
  children,
  hidden = false,
}: {
  title: string
  subtitle?: string
  action?: React.ReactNode
  children: React.ReactNode
  hidden?: boolean
}) {
  return (
    <section hidden={hidden} className={hidden ? 'hidden' : 'flex min-w-0 flex-col gap-3'}>
      <div className="flex items-start gap-3">
        <div className="flex flex-col gap-0.5">
          <h3>{title}</h3>
          {subtitle ? <span className="text-muted-foreground text-[11px]">{subtitle}</span> : null}
        </div>
        <div className="ml-auto">{action}</div>
      </div>
      {children}
    </section>
  )
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <span className="text-muted-foreground text-[11px]">{label}</span>
      <span className={cn('truncate text-[11px]', mono && 'font-mono')}>{value}</span>
    </div>
  )
}
