import { useEffect, useMemo, useState } from 'react'
import { Menu, Moon, PanelRight, Sun, Wifi, WifiOff, X, Globe, ArrowLeft, ArrowRight, Ellipsis } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { sample } from '@/lib/initial-source'
import { Composer } from '@/components/composer'
import { RunTimeline } from '@/components/transcript'
import { Conversation } from '@/components/conversation'
import { FilePanel } from '@/components/file-panel'
import { BrowserPanel } from '@/components/browser-panel'
import { ResourcesPage } from '@/components/resource-view'
import { CommandPalette } from '@/components/command-palette'
import { SettingsPage } from '@/components/settings-page'
import { SettingsDialog } from '@/components/settings-dialog'
import { Sidebar } from '@/components/workspace-sidebar'
import { ActivityRail } from '@/components/activity-rail'
import { ProjectDialog } from '@/components/project-dialog'
import './workbench-shell.css'
import { useWorkspace } from '@/lib/workspace'
import { useDraft } from '@/lib/drafts'
import { useTheme } from '@/lib/theme'
import { useMessageLayout } from '@/lib/message-layout'
import { useConversationScroll } from '@/lib/use-conversation-scroll'
import { useScopedPresentation } from '@/lib/use-scoped-presentation'
import { contextFrom, formatTokens, metricsFrom } from '@/lib/viewmodel'
import { useIsMobile, useMediaQuery } from '@/lib/viewport'

export default function App() {
  const workspace = useWorkspace()
  const fixture = workspace.source.kind === 'fixture' ? sample : null
  const live = workspace.source.kind === 'live'
  const sourceScope = workspace.source.kind === 'live' ? workspace.source.origin : workspace.source.kind
  const draftScope = `${sourceScope}/${workspace.selected.session || `project:${workspace.selected.project || 'none'}`}`
  const scrollScope = JSON.stringify([sourceScope, workspace.selected.project, workspace.selected.session])
  const choices = useScopedPresentation(scrollScope, () => new Map<string, boolean>())
  const { draft, setDraft, clear: clearDraft, selection } = useDraft(draftScope)
  const [rail, setRail] = useState(true)
  const [railTab, setRailTab] = useState<'browser' | 'run' | 'files'>('browser')
  const [page, setPage] = useState<'chat' | 'resources'>('chat')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const messageScroll = useConversationScroll(scrollScope, workspace.transcript?.parts, page === 'chat')
  const isMobile = useIsMobile()
  const isPanelOverlay = useMediaQuery('(max-width: 900px)')
  const [drawer, setDrawer] = useState(false)
  const [navigation, setNavigation] = useState(true)
  const [palette, setPalette] = useState(false)
  const [projectDialogFor, setProjectDialogFor] = useState<number | null>(null)
  // The run panel is a side column on desktop and an overlay on phones, so it
  // must start closed on narrow viewports instead of covering the transcript.
  useEffect(() => {
    if (isPanelOverlay) setRail(false)
  }, [isPanelOverlay])
  const [origin, setOrigin] = useState('')
  const [password, setPassword] = useState('')

  // `?live=1` connects explicitly (dev server proxy path). When the bundle is
  // hosted by a node, the entry document tells us so; then connecting to our
  // own origin is the only sensible default.
  useEffect(() => {
    if (sample) return
    const explicit = new URLSearchParams(window.location.search).get('live') === '1'
    if (explicit) {
      void workspace.connect(window.location.origin)
      return
    }
    let cancelled = false
    void fetch('/.well-known/rpi-work-entry', { headers: { accept: 'application/json' } })
      .then((response) => {
        if (!cancelled && response.ok && response.headers.get('content-type')?.includes('application/json')) void workspace.connect(window.location.origin)
      })
      .catch(() => {
        // Static hosting has no node entry. Remain empty and disconnected.
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // ⌘K / Ctrl-K opens the palette; the sidebar search button does the same.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // Escape must also work before the palette's deferred input focus lands.
      if (event.key === 'Escape') setPalette(false)
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        // Always open: a toggle can close a palette that focus already lost.
        event.preventDefault()
        if (document.querySelector('dialog:modal')) return
        setPalette(true)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const ready = workspace.presentationReady && (workspace.status === 'fixture' || workspace.status === 'ready')
  const theme = useTheme(workspace.presentationReady)
  const messageLayout = useMessageLayout()
  const dark = theme.choice.dark
  const openSettings = () => {
    setSettingsOpen(true)
    setDrawer(false)
    if (live) { void workspace.loadProviders(); void workspace.loadNodeInfo() }
  }
  const liveMetrics = workspace.runtime?.stats ?? null
  const liveContext = workspace.runtime?.context ?? null
  const metrics = useMemo(
    () => (ready ? metricsFrom(liveMetrics ?? fixture?.statsPayload ?? null) : null),
    [ready, liveMetrics, fixture],
  )
  const context = useMemo(
    () => (ready ? contextFrom(liveContext ?? fixture?.contextPayload ?? null) : null),
    [ready, liveContext, fixture],
  )
  const tokens = theme.tokens
  const contextText =
    ready && context
      ? `${formatTokens(context.used)} / ${formatTokens(context.window)} · ${context.percentText}`
      : '— / —'

  const title = workspace.sessions.find((entry) => entry.id === workspace.selected.session)?.title ?? '新对话'
  const model = workspace.runtime?.modelId ?? fixture?.node.model ?? '—'
  const modelLabel = model.split('/').slice(-1)[0]
  const thinking = workspace.runtime?.thinkingLevel ?? fixture?.node.thinking ?? '—'
  const modelOptions = workspace.models.map((model) => ({
    id: model.id,
    label: model.name || model.id.split('/').slice(-1)[0],
    provider: model.provider,
  }))
  const canCreate = live && ready && !workspace.creation.busy && !workspace.creation.uncertain
  const toolCount = workspace.blocks.filter((block) => block.kind === 'tool').length

  return (
    <div className={dark ? 'dark h-dvh' : 'h-dvh'} style={theme.style} data-palette={theme.choice.id} data-density={theme.choice.compact ? 'compact' : 'standard'}>
      <div className="workbench-shell bg-background text-foreground" data-navigation={navigation ? 'open' : 'closed'} data-message-layout={messageLayout.layout}>
        {((!isMobile && navigation) || (isMobile && drawer)) && (
        <div className={isMobile ? 'fixed inset-0 z-40 flex' : 'contents'}>
          {isMobile ? (
            <div
              className="absolute inset-0 bg-black/40"
              aria-hidden
              onClick={() => setDrawer(false)}
            />
          ) : null}
        <Sidebar
          key={sourceScope}
          scope={sourceScope}
          onClose={() => isMobile ? setDrawer(false) : setNavigation(false)}
          className={isMobile ? 'relative z-50 h-full shadow-xl' : undefined}
          projects={workspace.projects}
          sessions={workspace.sessions}
          project={workspace.selected.project}
          session={workspace.selected.session}
          onProject={(id) => { workspace.selectProject(id); setPage('chat'); if (isMobile) setDrawer(false) }}
          onSession={(id) => {
            workspace.selectSession(id)
            setPage('chat')
            if (isMobile) setDrawer(false)
          }}
          onSearch={() => setPalette(true)}
          onNewChat={canCreate && workspace.selected.project ? () => { setPage('chat'); if (isMobile) setDrawer(false); void workspace.createSession() } : undefined}
          onAddProject={canCreate ? () => { setPage('chat'); setProjectDialogFor(workspace.connectionId) } : undefined}
          creationHint={!live ? fixture ? '示例模式不创建真实数据；请连接节点' : '请先连接节点' : workspace.creation.busy ? '正在创建并核对，请勿重复提交' : workspace.creation.uncertain ? '上次创建结果尚未确认，请先核对' : '请先连接节点并选择项目'}
          nodeLabel={workspace.source.kind === 'live' ? workspace.source.origin.replace(/^https?:\/\//, '') : fixture ? 'fixture-node' : '未连接节点'}
          nodeState={live ? workspace.status === 'ready' ? 'online' : 'offline' : fixture ? 'fixture' : 'disconnected'}
          connection={
            live ? (
              <Button
                variant="outline"
                size="sm"
                className="h-7 w-full gap-2 text-[12px]"
                onClick={workspace.disconnect}
              >
                <WifiOff className="size-3.5" />
                断开节点
              </Button>
            ) : (
              <form
                className="flex flex-col gap-1.5"
                onSubmit={(event) => {
                  event.preventDefault()
                  void workspace.connect(origin.trim() || window.location.origin)
                }}
              >
                <Input
                  value={origin}
                  onChange={(event) => setOrigin(event.target.value)}
                  placeholder="http://127.0.0.1:13031"
                  className="h-7 font-mono text-[11px]"
                  aria-label="节点地址"
                />
                <Button
                  type="submit"
                  size="sm"
                  className="h-7 w-full gap-2 text-[12px]"
                  disabled={workspace.status === 'connecting'}
                >
                  <Wifi className="size-3.5" />
                  {workspace.status === 'connecting' ? '连接中…' : '连接节点'}
                </Button>
              </form>
            )
          }
        />
        </div>
        )}

        <ActivityRail page={settingsOpen ? 'settings' : page}
          onChat={() => setPage('chat')}
          onProjects={() => { if (isMobile) setDrawer(true); else setNavigation(true) }}
          onBrowser={() => { setRail(true); setRailTab('browser') }}
          onFiles={() => { setRail(true); setRailTab('files'); if (live) void workspace.loadChanges() }}
          onResources={() => { setPage('resources'); if (live) void workspace.loadResources() }}
          onSettings={openSettings}
        />
        <main className="wb-shell-main">
          <header className="wb-window-header">
            <div className="wb-window-marks" aria-hidden="true"><span /><span /><span /></div>
            <div className="wb-history-controls"><button type="button" className="wb-icon" aria-label="上一个会话" disabled title="会话导航历史待接入"><ArrowLeft aria-hidden="true" /></button><button type="button" className="wb-icon" aria-label="下一个会话" disabled title="会话导航历史待接入"><ArrowRight aria-hidden="true" /></button></div>
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                aria-label={isMobile ? '打开侧栏' : '切换会话侧栏'}
                onClick={() => isMobile ? setDrawer(true) : setNavigation(!navigation)}
              >
                <Menu className="size-4" />
              </Button>
            <div className="wb-window-title">
            <span className="sr-only">{workspace.selected.project}</span>
            <span className="truncate text-[14px] font-medium">{title}</span>
            <Badge variant="outline" className="ml-1 h-5 gap-1 px-1.5 text-[10px] font-normal">
              <span className={`size-1.5 rounded-full ${live && workspace.status === 'ready' ? 'bg-emerald-500' : workspace.status === 'fixture' ? 'bg-muted-foreground' : 'bg-amber-500'}`} />
              {workspace.status === 'boot' ? '加载中' : workspace.status === 'connecting'
                ? '连接中'
                : workspace.status === 'auth-required'
                  ? '需要密码'
                  : workspace.status === 'error'
                    ? '失败'
                    : live
                      ? '在线'
                      : fixture ? '示例 · 未连接' : '未连接'}
            </Badge>
            </div>
            <div className="wb-header-tools">
              <Button
                variant="ghost"
                size="icon"
                className="size-7"
                aria-label="切换主题"
                onClick={() => theme.setDark(!dark)}
              >
                {dark ? <Sun className="size-4" /> : <Moon className="size-4" />}
              </Button>
              <button type="button" className="wb-icon" aria-label="打开显示选项" title="显示选项" popoverTarget="wb-display-options"><Ellipsis aria-hidden="true" /></button>
              <div popover="auto" id="wb-display-options" className="wb-display-options" role="group" aria-label="工作台显示选项">
              <div className="flex flex-wrap items-center gap-1">
              <label className="flex items-center gap-1 text-[11px]">消息布局
                <select aria-label="消息布局" value={messageLayout.layout} onChange={event => messageLayout.setLayout(event.target.value === 'classic' ? 'classic' : 'reading')} className="border-input bg-background h-7 rounded-md border px-1.5 text-[11px]">
                  <option value="reading">阅读</option><option value="classic">经典</option>
                </select>
              </label>
              <select
                aria-label="选择主题"
                value={theme.choice.id}
                onChange={(event) => theme.setTheme(event.target.value)}
                className="border-input bg-background h-7 rounded-md border px-1.5 text-[11px]"
              >
                {(theme.presets.length > 0 ? theme.presets : [{ id: 'codex', name: 'Codex', description: '' }]).map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.name}
                  </option>
                ))}
              </select>
              <Button
                variant={theme.choice.compact ? 'secondary' : 'ghost'}
                size="sm"
                className="h-7 px-2 text-[11px]"
                aria-label="切换密度"
                onClick={() => theme.setCompact(!theme.choice.compact)}
              >
                {theme.choice.compact ? '紧凑' : '标准'}
              </Button>
              </div>
              <Button type="button" variant="ghost" aria-label="资源中心" popoverTarget="wb-display-options" popoverTargetAction="hide" onClick={() => { setPage(page === 'resources' ? 'chat' : 'resources'); if (live) void workspace.loadResources() }}>技能与插件</Button>
              <Button type="button" variant="ghost" aria-label="设置" popoverTarget="wb-display-options" popoverTargetAction="hide" onClick={openSettings}>设置</Button>
              </div>
              <Button
                variant={rail && railTab === 'browser' ? 'secondary' : 'ghost'}
                size="icon"
                className="size-7"
                aria-label="浏览器"
                onClick={() => { setRail(true); setRailTab('browser') }}
              >
                <Globe className="size-4" />
              </Button>
              <Button
                variant={rail && railTab === 'run' ? 'secondary' : 'ghost'}
                size="icon"
                className="size-7"
                aria-label="运行面板"
                onClick={() => {
                  setRail(true)
                  setRailTab('run')
                }}
              >
                <PanelRight className="size-4" />
              </Button>
              <Button
                variant={rail && railTab === 'files' ? 'secondary' : 'ghost'}
                size="sm"
                className="h-7 px-2 text-[11px]"
                aria-label="文件面板"
                onClick={() => {
                  setRail(true)
                  setRailTab('files')
                  if (live) void workspace.loadChanges()
                }}
              >
                文件
              </Button>
            </div>
          </header>

          <div className="wb-body">
            <div className="flex min-w-0 flex-1 flex-col">
              <div className="relative flex min-h-0 flex-1 flex-col">
              <div
                ref={messageScroll.scrollRef}
                onScroll={messageScroll.onScroll}
                className="wb-message-scroll min-h-0 flex-1 overflow-y-auto px-6 py-6"
                role="log"
                aria-live="polite"
                aria-label="会话记录"
              >
                {page === 'resources' ? (
                  <ResourcesPage
                    live={live}
                    resources={workspace.resources}
                    project={workspace.selected.project}
                    onRefresh={() => void workspace.loadResources()}
                    install={{
                      offers: workspace.offers,
                      bundle: workspace.bundle,
                      installing: workspace.installing,
                      error: workspace.installError,
                      onReview: (offer) => void workspace.reviewOffer(offer),
                      onCancel: workspace.cancelReview,
                      onConfirm: (scope) => void workspace.confirmInstall(scope),
                    }}
                  />
                ) : (
                <div className="wb-transcript mx-auto flex w-full max-w-[48rem] flex-col gap-8">
                  {workspace.creation.message ? <div className="wb-creation-message" role={workspace.creation.busy ? 'status' : 'alert'}><p>{workspace.creation.message}</p>{workspace.creation.uncertain ? <button type="button" disabled={workspace.creation.busy} onClick={() => void workspace.recheckCreation()}>核对创建结果</button> : null}</div> : null}
                  {workspace.dialogs.map((dialog) => (
                    <div
                      key={dialog.requestId}
                      className="border-amber-500/40 bg-amber-500/5 flex flex-col gap-2 rounded-lg border p-3"
                      role="group"
                      aria-label="待批准的请求"
                    >
                      <div className="flex items-center gap-2 text-[12px] font-medium">
                        <span className="size-1.5 rounded-full bg-amber-500" />
                        需要你的批准
                        {dialog.purpose ? (
                          <Badge variant="outline" className="h-4 px-1 text-[9px] font-normal">
                            {dialog.purpose}
                          </Badge>
                        ) : null}
                      </div>
                      <p className="text-[12px]">{dialog.question ?? '节点未提供问题文本。'}</p>
                      {!dialog.options.some(option => option.value !== null) ? <p className="text-muted-foreground text-[11px]">暂不支持此请求的回复形式；不会自动授权。</p> : null}
                      <div className="flex flex-wrap gap-2">
                        {dialog.options.map((option) => (
                          <Button
                            key={`${option.value}:${option.label}`}
                            size="sm"
                            disabled={!dialog.replyEnabled || option.value === null}
                            variant={option.label.includes('拒绝') ? 'outline' : 'default'}
                            className="h-6 px-2 text-[11px]"
                            onClick={() => { if (option.value !== null) void workspace.answerDialog(dialog.requestId, [option.value]) }}
                          >
                            {option.label}
                          </Button>
                        ))}
                      </div>
                    </div>
                  ))}
                  {workspace.status === 'error' ? (
                    <ErrorState message={workspace.error ?? '未知错误'} onRetry={() => void workspace.connect(origin.trim() || window.location.origin)} />
                  ) : workspace.status === 'auth-required' ? (
                    <form
                      className="border-input bg-card mx-auto flex w-full max-w-sm flex-col gap-3 rounded-xl border p-4"
                      onSubmit={(event) => {
                        event.preventDefault()
                        void workspace.submitpassword(password)
                      }}
                    >
                      <span className="text-[13px] font-medium">节点需要密码</span>
                      {workspace.error ? <p role="alert" className="text-destructive text-[12px]">认证未完成：{workspace.error}</p> : null}
                      <Input
                        type="password"
                        value={password}
                        onChange={(event) => setPassword(event.target.value)}
                        placeholder="节点密码"
                        aria-label="节点密码"
                      />
                      <Button type="submit">登录</Button>
                    </form>
                  ) : ready || workspace.status === 'disconnected' ? (
                    workspace.transcript && workspace.transcript.parts.length > 0 ? (
                      <Conversation key={scrollScope} view={workspace.transcript} choices={choices} details={fixture?.toolMeta} observed={live} />
                    ) : (
                      <EmptyState live={live} fixture={!!fixture} hasSession={!!workspace.selected.session} />
                    )
                  ) : (
                    <p className="text-muted-foreground text-[13px]">正在加载 work-viewmodel…</p>
                  )}
                </div>
                )}
              </div>
                {messageScroll.away && page === 'chat' ? (
                  <Button className="wb-latest-button absolute bottom-3 left-1/2 z-20 -translate-x-1/2 shadow-md" size="sm" variant="secondary" onClick={messageScroll.jumpToLatest}>
                    {messageScroll.unread ? '有新消息 · 回到底部' : '回到底部'}
                  </Button>
                ) : null}
              </div>
              {page === 'chat' ? (
              <>
              {workspace.recovery && (workspace.recovery.count > 0 || workspace.recovery.unreceipted || workspace.recovery.message) ? (
                <div className="mx-4 flex items-center gap-3 rounded-lg border px-3 py-2 text-xs" role="status">
                  <p className="text-muted-foreground min-w-0 flex-1">
                    {workspace.recovery.message || (workspace.recovery.unreceipted ? '部分请求未取得可核对回执，保持锁定；不会自动重发。' : '请求结果尚未核实；只读核对不会重新执行，也不会自动发送草稿。')}
                  </p>
                  <Button size="sm" variant="outline" disabled={!workspace.recovery.canCheck} onClick={() => void workspace.recheckSession()}>
                    {workspace.recovery.checking ? '核对中…' : '只读核对'}
                  </Button>
                </div>
              ) : null}
              <Composer
                draft={draft}
                selection={selection}
                onDraft={setDraft}
                sendDisabledReason={!live ? fixture ? '示例模式不会发送消息' : '请先连接节点' : !ready ? '节点尚未就绪' : !workspace.selected.session ? '请先在侧栏创建真实会话；当前草稿会保留' : !workspace.controls?.send ? workspace.controls?.reason || '未取得当前会话可写观察' : undefined}
                queueDisabledReason={!workspace.controls?.queue ? workspace.controls?.reason || '当前会话未声明入队能力' : undefined}
                abortDisabled={!workspace.controls?.abort}
                modelDisabled={!workspace.controls?.model}
                thinkingDisabled={!workspace.controls?.thinking}
                selectedModel={workspace.runtime?.modelId && workspace.runtime.provider ? { id: workspace.runtime.modelId, provider: workspace.runtime.provider } : undefined}
                context={context ?? { used: null, window: null, percent: null, threshold: null, percentText: '—' }}
                onSend={() => {
                  void workspace.send(draft).then((ok) => {
                    if (ok) clearDraft()
                  })
                }}
                onQueue={
                  live
                    ? () => {
                        void workspace.queueMessage(draft).then((ok) => {
                          if (ok) clearDraft()
                        })
                      }
                    : undefined
                }
                onAbort={() => void workspace.abort()}
                busy={workspace.busy}
                error={workspace.sendError}
                modelLabel={modelLabel}
                thinking={thinking}
                modelOptions={live ? modelOptions : undefined}
                thinkingOptions={live ? workspace.thinkingLevels : undefined}
                onModel={live ? (id, provider) => {
                  const model = workspace.models.find((entry) => entry.id === id && entry.provider === provider)
                  if (model) void workspace.setModel(model)
                } : undefined}
                onThinking={live ? (level) => void workspace.setThinking(level) : undefined}
              />
              </>
              ) : null}
            </div>

            <div className={!rail ? 'hidden' : isPanelOverlay ? 'fixed inset-0 z-40 flex justify-end' : 'contents'}>
              {isPanelOverlay ? (
                <div
                  className="absolute inset-0 bg-black/40"
                  aria-hidden
                  onClick={() => setRail(false)}
                />
              ) : null}
              <aside
                className={isPanelOverlay
                  ? railTab === 'browser' ? 'bg-background relative z-50 h-full w-[94vw] max-w-[480px] border-l' : 'bg-background relative z-50 h-full w-[86vw] max-w-[320px] border-l'
                  : railTab !== 'run' ? 'workbench-browser-inspector shrink-0 border-l' : 'w-[300px] shrink-0 border-l'}
                data-testid="run-panel"
              >
                <div className="flex h-full flex-col">
                  <div className={railTab === 'browser' ? 'hidden' : 'flex h-11 items-center justify-between border-b px-3 text-[12px] font-medium'}>
                    {railTab === 'files' ? '文件' : '运行'}
                    <span className="flex items-center gap-2">
                      <span className="text-muted-foreground font-normal">{live ? 'node' : fixture ? 'fixture' : '未连接'}</span>
                      {isMobile ? (
                        <Button variant="ghost" size="icon" className="size-6" aria-label="关闭面板" onClick={() => setRail(false)}>
                          <X className="size-3.5" />
                        </Button>
                      ) : null}
                    </span>
                  </div>
                  <div className="real-browser-host" hidden={railTab !== 'browser'}>
                    <BrowserPanel onClose={() => setRail(false)} />
                  </div>
                  {railTab === 'files' ? (
                    <FilePanel
                      key={`${workspace.connectionId}:${workspace.selected.project}:${workspace.selected.session}`}
                      live={live && Boolean(workspace.selected.project)}
                      presentationReady={workspace.presentationReady}
                      changes={workspace.changes}
                      fileRead={workspace.fileRead}
                      onRefresh={() => void workspace.loadChanges()}
                      onOpen={(path, staged) => void workspace.openChange(path, staged)}
                      onOpenSource={(path) => void workspace.openFile(path)}
                      listDir={workspace.listDir}
                    />
                  ) : railTab === 'run' ? (
                  <div className="min-h-0 flex-1 overflow-y-auto p-3">
                    <RunTimeline steps={fixture?.runSteps ?? []} />
                    <Separator className="my-3" />
                    <div className="flex flex-col gap-2 text-[12px]">
                      <Row label="模型" value={model} />
                      <Row label="思考" value={thinking} />
                      <Row label="工具" value={ready ? `${toolCount} 次` : '—'} />
                      <Row label="缓存命中" value={metrics?.cacheText ?? '—'} />
                      <Row label="成本" value={metrics?.cost == null ? '—' : `$${metrics.cost.toFixed(4)}`} />
                      <Row label="消息" value={`${workspace.messageCount} 条`} />
                    </div>
                    <Separator className="my-3" />
                    <div className="flex flex-col gap-2 text-[12px]">
                      <div className="flex items-center justify-between">
                        <span className="text-muted-foreground">队列 {workspace.queue.count}</span>
                        {workspace.queue.count > 0 ? (
                          <button
                            className="text-muted-foreground hover:text-foreground text-[11px]"
                            disabled={!workspace.controls?.clearQueue}
                            onClick={() => void workspace.clearQueue()}
                          >
                            清空
                          </button>
                        ) : null}
                      </div>
                      {workspace.queue.items.map((item) => (
                        <div key={item.entryId} className="bg-muted/40 flex items-center gap-2 rounded-md px-2 py-1">
                          <span className="truncate">{item.text}</span>
                          <button
                            aria-label={`取消排队 ${item.text}`}
                            className="text-muted-foreground hover:text-destructive ml-auto text-[11px]"
                            disabled={!workspace.controls?.cancelQueued}
                            onClick={() => void workspace.cancelQueued(item.entryId)}
                          >
                            ×
                          </button>
                        </div>
                      ))}
                    </div>
                    <Separator className="my-3" />
                    <details className="text-[11px]">
                      <summary className="text-muted-foreground cursor-pointer">Rust 主题 token</summary>
                      <pre className="bg-muted/40 mt-2 max-h-40 overflow-auto rounded-md p-2 font-mono text-[10px]">
                        {JSON.stringify(
                          { bg: tokens.bg, accent: tokens.accent, line: tokens.line, 'body-text': tokens['body-text'] },
                          null,
                          2,
                        )}
                      </pre>
                    </details>
                  </div>
                  ) : null}
                  {railTab !== 'browser' ? <div className="text-muted-foreground border-t px-3 py-2 text-[11px]">上下文 {contextText}</div> : null}
                </div>
              </aside>
            </div>
          </div>

          {settingsOpen ? <SettingsDialog onClose={() => setSettingsOpen(false)} appearance={
            <section>
              <h3>外观</h3>
              <p className="text-muted-foreground mb-6 text-sm">仅影响本设备显示，不改变节点配置或运行中的任务。</p>
              <label className="wb-settings-preference">颜色模式<select aria-label="颜色模式" value={dark ? 'dark' : 'light'} onChange={event => theme.setDark(event.target.value === 'dark')}><option value="light">浅色</option><option value="dark">深色</option></select></label>
              <label className="wb-settings-preference">选择主题<select aria-label="选择主题" value={theme.choice.id} onChange={event => theme.setTheme(event.target.value)}>{(theme.presets.length ? theme.presets : [{ id: 'codex', name: 'Codex' }]).map(preset => <option key={preset.id} value={preset.id}>{preset.name}</option>)}</select></label>
              <label className="wb-settings-preference">界面密度<select aria-label="界面密度" value={theme.choice.compact ? 'compact' : 'standard'} onChange={event => theme.setCompact(event.target.value === 'compact')}><option value="standard">标准</option><option value="compact">紧凑</option></select></label>
              <label className="wb-settings-preference">消息布局<select aria-label="消息布局" value={messageLayout.layout} onChange={event => messageLayout.setLayout(event.target.value === 'classic' ? 'classic' : 'reading')}><option value="reading">阅读</option><option value="classic">经典</option></select></label>
            </section>
          }>{active => <SettingsPage
            key={`${workspace.connectionId}:${scrollScope}`}
            active={active}
            live={live}
            origin={workspace.source.kind === 'live' ? workspace.source.origin : null}
            project={workspace.selected.project}
            session={workspace.selected.session}
            providers={workspace.providers}
            revision={workspace.providerRevision}
            error={workspace.providerError}
            onRefresh={() => void workspace.loadProviders()}
            authorization={workspace.authorization}
            authorizationError={workspace.authorizationError}
            onIssueAuthorization={() => void workspace.issueAuthorization()}
            onSaveKey={workspace.saveProviderKey}
            notice={workspace.providerNotice}
            nodeInfo={workspace.nodeInfo}
          />}</SettingsDialog> : null}
          <footer className="text-muted-foreground flex h-7 shrink-0 items-center gap-4 border-t px-4 text-[11px] tabular-nums">
            <span>↑ {metrics?.inputText ?? '—'}</span>
            <span>↓ {metrics?.outputText ?? '—'}</span>
            <span>缓存 {metrics?.cacheText ?? '—'}</span>
            <span>成本 {metrics?.cost == null ? '—' : `$${metrics.cost.toFixed(4)}`}</span>
            <span>上下文 {context?.percentText ?? '—'}</span>
            <span>工具 {toolCount} / 失败 0</span>
            <Tooltip>
              <TooltipTrigger className="ml-auto cursor-default">
                ReUI · Codex 布局原型 · rules from Rust
              </TooltipTrigger>
              <TooltipContent>行序、工具目标、节点协议、指标格式都来自 Rust（wasm）</TooltipContent>
            </Tooltip>
          </footer>
        </main>
      </div>
      {projectDialogFor === workspace.connectionId && live && workspace.status === 'ready' ? <ProjectDialog
        key={workspace.connectionId}
        nodeLabel={workspace.source.kind === 'live' ? workspace.source.origin : ''}
        busy={workspace.creation.busy}
        uncertain={workspace.creation.uncertain}
        message={workspace.creation.message}
        onCreate={workspace.createProject}
        onRecheck={workspace.recheckCreation}
        onClose={() => setProjectDialogFor(null)}
      /> : null}
      <CommandPalette
        open={palette}
        projects={workspace.projects}
        sessions={workspace.sessions}
        onClose={() => setPalette(false)}
        onSelect={(projectId, sessionId) => {
          workspace.selectProject(projectId)
          workspace.selectSession(sessionId)
          if (isMobile) setDrawer(false)
        }}
      />
    </div>
  )
}

function EmptyState({ live, fixture, hasSession }: { live: boolean; fixture: boolean; hasSession: boolean }) {
  return (
    <div className="text-muted-foreground mx-auto flex flex-col items-center gap-2 py-24 text-center text-[13px]">
      <span className="text-[15px] font-medium">{hasSession ? '这个会话还没有消息' : '尚未选择会话'}</span>
      <span>{!hasSession && live ? '请在侧栏点击“新对话”创建真实会话；当前草稿会保留。' : live ? '从节点读取到 0 条消息。' : fixture ? 'fixture 会话没有内容。' : '请先连接节点；当前草稿会保留，不会显示示例数据。'}</span>
    </div>
  )
}

function ErrorState({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="border-destructive/40 bg-destructive/5 mx-auto flex w-full max-w-md flex-col gap-3 rounded-xl border p-4">
      <span className="text-[13px] font-medium">连接节点失败</span>
      <p className="text-muted-foreground text-[12px] break-words">{message}</p>
      <Button size="sm" onClick={onRetry} className="self-start">
        重试
      </Button>
    </div>
  )
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="truncate font-mono text-[11px]">{value}</span>
    </div>
  )
}
