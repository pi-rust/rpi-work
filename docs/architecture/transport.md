# RPI Work 架构与传输边界

本文保留传输、消息和生命周期的详细约束。系统全貌见[架构总览](overview.md)，实际模块见[项目结构](../guides/project-structure.md)，WebSocket 协商与操作回执见[实时通道](realtime.md)，原生资源见[资源服务](resources.md)。

`work-node-runtime::workspace::LocalWorkspace` is the authoritative workspace owner.
`work-client::LocalWorkspace` is a local access adapter holding that same owner;
`WorkspaceClient` selects local access or authenticated remote HTTP/WebSocket. The
HTTP server receives the existing node owner rather than creating a second one.

Physical boundary: rpi capabilities live in `rpi/extcore`; the HTTP adaptation
that serves rpi capabilities lives in `rpi-http`. Work owns its node management
and dedicated panels (`crates/components/work-http-component`, `crates/components/work-component-api`).
Work's multi-project owner is **not** an implementation of `rpi-http`, and Work
does not download, select or supervise an rpi HTTP extension as its desktop host
runtime. A new rpi HTTP plugin does not automatically implement Work's existing
remote workspace protocol; interoperability must be declared and verified
explicitly. See [component boundaries](component-composition.md).

```text
Desktop UI → WorkspaceClient::Local → local access adapter ────────────┐
Hosted Web / Android / remote UI → WorkClient → work-node-service HTTP/WebSocket ─┤
                                                                    ▼
                                   work-node-runtime::workspace::LocalWorkspace
                                   ├── projects/catalog/files/Git
                                   ├── RPI → rpi-extcore → rpi stdio RPC
                                   └── ACP → node-owned Agent processes
CLI → WorkClient → existing owner's authenticated HTTP/WebSocket
GUI / CLI serve → work-host → one local owner + optional HTTP entrance
Explicit CLI rpc/acp diagnostics → AgentClient → separate native Agent
```

## Crate boundaries

- `work-node-api`: DTOs, typed `NodeApi`, `SessionAccess`,
  `SessionConfiguration`, resource and realtime contracts; no UI or host.
- `work-core`: re-exports node contracts and adds client event normalization,
  run projection and inert extension designs.
- `work-node-runtime`: actual workspace owner, native RPC/ACP runtimes, backend
  bindings, persistence, native resources and source authoring.
- `work-client`: local/remote adapters, HTTP/WebSocket, snapshot recovery,
  pagination and client projection. Native RPC/ACP facades delegate to the node
  runtime rather than creating another execution implementation.
- `work-node-service`: authenticated HTTP/WebSocket and optional Web assets
  over an existing owner; security helpers live in `work-http-runtime`.
  Integration tests and examples live beside the implementation. The production
  server layer does not depend on `work-client`.
- `work-node-host`: actual GUI-independent startup, data lock, component binding,
  HTTP configuration and ordered shutdown. `work-host` re-exports application
  entry points and wraps the same owner as a `WorkspaceClient::Local`.
- `work-update`: signed release discovery and platform update operations.
- `apps/work-ui`: shared Dioxus desktop/Web/Android UI and platform entry gates.
  `apps/work-cli` defaults to typed existing-node operations, with foreground
  hosting and explicitly selected native diagnostics.

`NodeApi` is an in-process typed application port, not a network service,
permission grant or replacement for the owner. Its futures need not be `Send`.
Raw commands, host lifecycle and transport-specific QR operations are outside
that port. HTTP handlers currently dispatch to the supplied node workspace.

This describes current source, not a newly shipped binary or an installed-app
upgrade. See [project structure](../guides/project-structure.md),
[desktop HTTP](../guides/desktop-http.md),
[node connection](../guides/node-connection.md) and
[extension boundaries](extensions.md) for the corresponding interfaces.

## Client entry and authentication

- Default owning Desktop prepares its local owner through `work-host` and
  accesses it in-process without an HTTP login. `work-host::http` attaches
  `work-node-service` to that same owner; `app/local_http.rs` is only a UI adapter.
  Explicit `--connect`/remote environment startup does not create a local owner. With no
  explicit configuration it listens on `0.0.0.0:13031`; a missing sharing preference
  defaults to enabled. Saved disabled or invalid/unreadable preferences remain
  disabled. Explicit bind/proxy configuration is authoritative; no firewall rules
  are changed. A workspace-global, one-shot startup offer issues an Android App
  authorization QR only after credential-free health/node-identity verification.
- Hosted Web is identified by `/.well-known/rpi-work-entry` when assets are
  configured. Authentication precedes workspace mounting and catalog reads;
  an existing valid Cookie may restore access. The initial owner is the page's
  origin, not another node selected in device history. Other nodes remain
  available after entering the workspace.
- Independent Web receives an explicit 404 at that metadata path and retains
  node selection and authentication. Discovery failures do not bypass the gate.
- Android native entry requires a dedicated single-use authorization QR and
  explicit target confirmation. A phone browser still uses the Web password
  gate; responsive layout does not select an authentication scheme.

The authorization ticket expires after 120 seconds, is redeemable once and is
bound to the issuing node's sharing generation. Ordinary address-sharing QR
codes do not authorize access. Host/Origin checks, request markers and rate
limits remain enforced. Closing sharing revokes external access without
stopping owner tasks. Native App authorization needs only the authenticated API,
not browser assets. Its LAN address is resolved separately from the ordinary Web
share address and added as an exact allowed Host. Web assets remain opt-in via
`RPI_WORK_HTTP_ASSETS`; a listening API port alone is not a complete browser entry.
The startup authorization component survives Settings/layout remounts and never
renews a ticket on expiry, failure, dismissal or reconnect; explicit regeneration
is required. This source/build behavior does not replace an already-running app.

## Protocol boundaries

`work-core::AgentDescriptor` identifies the protocol and transport and carries
an operation-level capability set. ACP and native RPC support prompts and
streaming, but do not automatically provide HTTP workspace APIs for projects,
files, or Git diffs. Native RPC does expose its own session-scoped history,
messages, queue, model/tool configuration, and provider-auth commands; these
are separate from HTTP workspace resources and are advertised only when the
ready frame lists the corresponding command.

Compatibility facade and CLI callers use `AgentDescriptor::supports` or
`AgentClient::require`. Workspace UI operations instead use the bound session's
`SessionAccess` / `SessionConfiguration` through typed application methods;
node-global capabilities and native handle presence are not fallbacks.
A transport returns `ClientError::Unsupported` when a command is outside its
negotiated surface; it must not fabricate a successful response.

`WorkspaceClient::local_rpc` wraps the node workspace after
`work-node-runtime::local::LocalRuntime` resolves `rpi` and holds the data lock.
`work-host::http` passes `workspace.node_owner()` to `HttpServer::start`.
This opens the embedded listener when configuration permits, but never spawns
or requires standalone `rpi-http`, and local UI access does not authenticate
over loopback. Local workspace resources are Rust service operations, not
invented project/file/Git commands on the RPI RPC wire.

## Remote HTTP and WebSocket

HTTP remains the authentication and resource plane. `WorkClient` uses
the WebSocket-only client: authenticated transport discovery validates the
WebSocket before session requests and events use it. There is no realtime HTTP
fallback; network, auth and protocol errors remain errors.
Mutations use owner-scoped bounded operation receipts. An unknown outcome must
be queried using the original reference, never automatically replayed. Receipts
are in-memory and owner-epoch-scoped, not durable exactly-once execution.
See [realtime protocol](realtime.md) for discovery, authentication, reservation,
execution and reconnect boundaries.

## Native RPI RPC

`RpiRpcClient` validates the `ready` frame's `protocol=rpi` and reflects its
`piCompatibilityTarget` and `supportedCommands` into the live
`AgentDescriptor`. A command omitted by the runtime is not advertised as
supported by the client.

The client starts `rpi --mode rpc` with piped stdin/stdout/stderr. One reader
owns stdout and matches JSONL responses by numeric request id. Notifications are
broadcast as the existing `WireEvent` type, so the shared run projection keeps
`agent_settled` as the terminal signal. Stderr is drained separately and never
participates in protocol parsing.

## ACP

The native `work-node-runtime::runtime::AcpClient` starts an ACP stdio executable and performs ACP v1
`initialize`/`session/new` negotiation. It supports text prompts, cancellation,
streaming `session/update` notifications, permission requests, session config
options, and explicit JSON-RPC responses to agent-initiated requests. The
client intentionally negotiates no ACP filesystem or terminal client capability
because those request handlers are not implemented in the runtime;
agent-initiated permission requests remain explicitly handled through the
permission-response facade. `work-client::AcpClient` delegates to this runtime;
the node workspace owns managed ACP bindings and processes. The sibling
`rpi-acp` package remains an ACP server and is not imported as an implementation
dependency.

### Permission response safety

`session/request_permission` is converted into a single-choice permission card.
The original options and JSON-RPC request ID (including string versus number)
are retained. `PendingDialog::acp_permission_result` maps one explicitly chosen
label back to the exact advertised `optionId`; missing, unknown, or ambiguous
choices fail instead of defaulting to allow. Cancellation sends the ACP
`{"outcome":{"outcome":"cancelled"}}` result and never selects an option.

`AcpClient` tracks agent-initiated permissions separately from client requests,
validates selected IDs against that request's options, and rejects unknown or
already-answered request IDs. A successfully written answer emits
`ui_dialog_closed` to remove the pending card. This is not an operation-success
or turn-settlement signal. The fixture's permission-wait mode sends no prompt
completion until it receives a valid response; tests cover allow/reject/cancel,
string/number IDs, bidirectional ID collisions and duplicate/forged answers.

`AgentClient` exposes protocol-neutral `open_session`, `prompt`, `abort`,
`subscribe`, `stop`, and permission-response operations through an opaque
`AgentSession`. Session config selection is also normalized through
`AgentSession::set_config_option` for negotiated `model` and `thinking` options;
ACP sends `session/set_config_option`, while RPC maps these to the native
`set_model`/`set_thinking_level` commands. `AgentClient::load_session` and `resume_session` are available only when the
dedicated `session_load`/`session_resume` capabilities are negotiated. HTTP
implements `session_load` through its authenticated session lookup, while ACP
delegates to `session/load`/`session/resume`; RPC intentionally exposes neither
capability. Unsupported operations return explicit `Unsupported`. Unknown
options likewise return `ClientError::Unsupported`. HTTP maps `open_session` to
`create_session`, RPC maps it to the current `get_state` session, and ACP maps
it to `session/new`.

### Capability and operation boundaries

ACP `loadSession` enables only `session_load`, not the facade's random-access
`history`/`messages` APIs. Receiving streamed tool calls does not advertise a
tool catalog or `set_active_tools`. Accordingly ACP keeps `history`, `tools`,
`models`, and `thinking_levels` discovery flags false for the current adapter.

`AgentClient::supports_config_option(session_id, id)` is separate from catalog
capabilities. ACP records select-option IDs per session from new/load/resume
and successful setter responses, replaces that session's previous options, and
sends only a negotiated ID. The thinking/thought_level alias maps back to the
actual advertised ID; an unknown session or withdrawn option is Unsupported.
The explicit facade/CLI setter remains usable without claiming a model catalog.
The adapter's catalog controls remain unsupported for generic ACP rather than
inventing choices. Workspace UI reads the bound session's
`SessionConfiguration` and renders its negotiated `configOptions`; it does not
infer a model catalog from the node-global descriptor.

`AgentClient::supports_command` adds operation-level gating to facade reads and
controls. RPC checks the actual ready command list; a broad `history=true` does
not enable an omitted `compact` or `resume_pending`. ACP allows only the
implemented prompt/cancel command mappings; config uses its dedicated setter.
The adapters include command checks for discovery and context controls. The
workspace also exposes `SessionAccess` for the effective operations of the
bound runtime and typed dialog replies; unknown access must not fall back to
node-global capabilities or a direct raw-RPC lane. Neither capability contract
is an authorization grant.

`AgentSession::connect` provides an `AgentSessionConnection` that reduces
normalized native events into the same `SessionEvent`/`RunProjection` shape as
HTTP sessions. It intentionally starts without HTTP snapshot/history semantics;
those remain an explicit HTTP capability until an agent protocol supplies them.
The connection reuses the shared message identity normalization so ACP chunks
without a message id remain stable across a turn.

`AgentClient` also exposes protocol-neutral session-scoped reads and controls
for native RPC (`state`, `messages`, `history`, `session_branch`, `queue`,
`models`, `thinking_levels`, `tools`, `stats`, and provider auth status). Each
method checks the negotiated capability before dispatching and returns
`Unsupported` instead of guessing an HTTP equivalent. These methods are adapter APIs, not the default desktop workspace backend. Project/file/Git
operations come from local Rust services rather than the RPC wire protocol.

A listed `get_session_branch` command does not by itself promise an unfiltered
branch. The legacy `../pi-rust` schema permits that read, while the strict
custom-entry-preview runtime rejects queries without a non-empty `customTypes`
array. `AgentClient::session_branch` translates that specific schema rejection
to `Unsupported { operation: "unfiltered_session_branch", .. }`; other errors
remain errors. It must not substitute a custom-only preview for compaction
entries or report an empty summary as a successful full-branch read. The live
smoke distinguishes these contracts explicitly and, for the preview-only
runtime, verifies a caller-specified `customTypes` query separately.

## Bounded transcript windows

`MESSAGE_PAGE_SIZE` is 48. Session connection and reconnect request that limit
only when the bound session's `SessionAccess::snapshot_limit` is true; otherwise
they read a full snapshot, including ACP sessions that reject limited snapshots.
The connection retains that policy for run-boundary recovery without advancing
the WebSocket event cursor to a later snapshot's `lastSeq`. The existing unrestricted
`snapshot()` API remains available; old servers may ignore the requested limit.
The published extcore snapshot options accept `limit`, not `beforeId`.

Older messages use the separate typed `WorkspaceClient::message_page()` command
path: `get_messages { limit, beforeId }`. This is transcript history, **not** the
`get_history` run-record API. The response must echo `beforeId`, contain unique
real `entryId` values with matching `firstId`/`lastId`, and advance the boundary.
Missing/inconsistent metadata or an ignored cursor returns
`ClientError::Unsupported`, rather than looping over the latest page. A local
ACP binding rejects this operation before sending any native request. The UI
also requires an advertised `get_messages` command and usable snapshot cursor;
`AgentConnection::Native` supplies no page metadata and remains disabled.

The UI retains loaded prefixes across live updates, with tail values winning
ID conflicts. Settlement snapshots are bounded suffixes: retained pre-boundary
messages stay before the suffix; omitted recent live messages stay after it.
Node/session scope, connection generation and window revision isolate late page
responses, including a reconnect with the same cursor. A reconnect replaces the
window with the new snapshot rather than persisting old pages across branches.
This is not native history replay, durable resume, or list virtualization.

## Native message identity and compact deltas

RPI RPC lifecycle events carry the durable session entry identity in the **event
envelope** `messageId`, not necessarily inside `message`. Default compact delta
frames omit `message` entirely, carrying only `eventType`, `contentIndex`, `delta`
and the nested assistant event. Treating starts/ends as anonymous while decoding
their deltas under the envelope ID creates two visible replies from one message.
Older nested-ID-only fixtures did not exercise this real wire contract.

`MessageIdentity` now resolves nonblank envelope `messageId`/`entryId` before
legacy nested identity and supplies an internal assistant identity shell for
compact updates. It does not synthesize content or use a WebSocket event/command `id`
as message identity. A first real ID may promote the current anonymous segment;
changed authoritative IDs still delimit different messages. HTTP and direct
native delivery share the same retained-body/projection owner-transfer routine,
with stale sequences rejected before identity mutation.

Snapshot coverage uses that normalized identity for the entire compact segment,
while comparing the raw complete body for legacy metadata fallback. It never
uses equal public text as a global deduplication key: two real IDs with identical
text/timestamps remain separate. Reopening/reconnecting must not revive the
anonymous history row or a detached compact delta projection.

`duplicate_regression_tests.rs`, native synthetic-stream tests and
`verify-compact-message-identity.cjs` cover the actual envelope format. The browser
fixture reproduces two visible replies on the pre-fix bundle and verifies one
owner during streaming, end, settlement, reopen and reconnect after the fix.
This does not replace live-backend/native desktop/Android verification or prove
that an already-running old client has been updated.

Native `turn_end` also embeds the final assistant body, without publishing a
new message entry. Only `message_start` / `message_update` / `message_end` may
merge transcript bodies. Boundary summaries still apply their normal control
semantics, but must not mint a `live-assistant-*` row or overwrite an existing
message. Otherwise a full turn creates a second transient reply that survives
settlement and disappears only on reopen. `native-short-reply.json` captures a
complete installed-runtime turn from an isolated loopback canned provider,
including `turn_end`; client and `verify-turn-summary-identity.cjs` regressions
cover this boundary. This is not text-based deduplication or Android-device
acceptance.

## Message image content

RPI image blocks have the canonical shape
`{ type: "image", mimeType: "image/png", data: "<base64>" }`.
The UI renders supported inline raster blocks in user, assistant and tool-result
messages; it does not infer file URLs, fetch remote images, upload attachments,
or change prompt capabilities. Direct native connections now use the same live
message merge as HTTP connections, rather than appending duplicate partial rows.
Content-less public text deltas patch an existing image-bearing row so history
projection deduplication cannot hide a later caption. Stale native sequences are
rejected before mutating message state.

Assistant normalization emits only public text, not encoded images or private
reasoning. Image-bearing tool results retain a structured internal string until
the timeline separates raster blocks from display/copy text. Trace sanitization
replaces image objects with a placeholder, also handling bounded JSON-string
wrappers; oversized structured strings are omitted from trace display. These
internal payloads are not workspace uploads or durable media storage.

PNG/JPEG/GIF/WebP data is checked before creating a Blob image: strict base64,
10 MiB decoded limit, matching signature, checked dimensions (16,384 per side,
40 million pixels) and animation frame budgets. A supported browser bitmap
decoder provides an additional decode check; Blob URLs and decoded bitmaps are
released on update/disposal and stale callbacks cannot overwrite a newer entry.
The viewer uses inert images inside a native dialog, never user SVG/HTML.

ACP image chunks remain outside the current adapter contract. Native synthetic
message tests and HTTP/WebSocket browser fixtures validate only their stated layers;
installed-runtime handshakes do not prove native image rendering, desktop E2E,
Android E2E or live HTTP backend acceptance.

## Local attachment send guard

The inspected RPI RPC `Command::Prompt` exposes a string `message` (with string
aliases), not image input blocks. The current HTTP/extcore and local workspace
prompt paths retain that text-only command shape. Adding unadvertised `images`
fields is not an implementation: unknown fields can be silently ignored. Real
image sending remains blocked pending an explicit backend contract and capability,
acceptance semantics, storage/access control and history restoration.

Local composer files therefore require one-attempt text-only confirmation before
main prompt/queue dispatch. Its metadata-only token contains scope, session
generation, draft revision, file IDs/names and queue delivery intent, never file
contents. Button, Enter and pending-Enter replay share the same gate; reads still
in flight block sending even before any file chip exists. Confirmation is consumed
synchronously once and revalidated before dispatch. Draft/file/scope/generation,
connection, page/tab or prompt/queue/delivery changes invalidate it without sending.
There is no remembered opt-out; failed requests require fresh consent. Cancellation
and failures retain the draft and local files; an accepted text request does not
remove files. Ordinary text-only sends and independent BTW/queue editors keep their
existing behavior. No new prompt fields, upload endpoint or ACP image capability
is introduced. Explicit text quotation remains user-authored draft content.

Rust metadata/consent tests and `verify-attachment-send-guard.cjs` cover this UI
safety boundary using an isolated HTTP/WebSocket fixture; they do not demonstrate real
attachment transfer or native desktop/Android/live-backend E2E.

## Desktop native workspace

`work-node-runtime::workspace::LocalWorkspace` owns the app catalog, per-project
`rpi-extcore::service::Service` instances and ACP backend manager. Projects,
folders, files and Git are local Rust operations. RPI execution/control uses
`rpi --mode rpc` stdio children owned through extcore; selected ACP bindings use
node-owned ACP runtimes. Workspace session lists, snapshots and subscriptions
come from this owner, not invented RPC methods or a client-owned parallel host.
The lower-level RPC adapter does not imply cross-process `session/load` or
`session/resume` support.

Native folder selection uses the OS picker and accepts arbitrary paths the
current user can access. Creating a project uses the exact selected/entered
absolute path. Remote folder browsing and creation retain the node's root and
API restrictions; they do not gain access to the desktop filesystem.

`rpi-work://local` is a device-local UI key, not a network endpoint or a
cross-device node ID. Web,
Android and explicit remote connections use a reachable authenticated HTTP/WebSocket
entry, which may be the desktop's embedded `work-node-service` sharing that same
owner. A separately started standalone server is another owner; it does not
synchronize by sharing a directory or copying session files. HTTP routes expose
backend discovery, bound-session access/configuration and owner-side session
creation; a remote client never launches an ACP process on its own device.
There is no implicit HTTP fallback or desktop `RPI_WORK_TRANSPORT=http` / `acp`
selector; those environment choices remain explicit CLI alternatives.

### Local data and migration

Let `<data>` be the existing platform `rpi-work/node` directory, or
`RPI_WORK_DATA_DIR`. The same `<data>/node.lock` protects ownership. The native
catalog is `<data>/rpc/projects.json`; each project registry is
`<data>/rpc/projects/<id>/sessions.json`. The lazily created model/auth control
context is separate from user projects under `<data>/rpc/control`.

`crates/node/work-node-runtime/src/workspace/migration.rs` imports accessible project aliases from
`<data>/projects.json` and `<data>/node/projects.json`, deduplicated by canonical
project identity. It imports missing entries and preferences from
`<data>/sessions.json` and `<data>/node/sessions.json` only when `projectId`
matches an imported/registered project. Existing target entries are retained;
unavailable paths and all legacy source files are preserved. No transcript is
rewritten, and this is not global native-history discovery or a scan of all
user session roots.

Component packages live under `<data>/components`, and staged listener settings
in `<data>/http-component.json`. Native authoring source/provenance is stored
under `<project>/.rpi/work-authoring`; exact native configuration, transcript and
resource operation recovery remain extcore/rpi facts, not UI desired pins. See
[resource ownership](resources.md).

ACP bindings and checkpointed events are stored in
`<data>/rpc/agent-sessions.json`; this is node-owned history, not a client cache.
`<data>/node-id` stores node identity and `<data>/sharing.json` stores the
owner's sharing preference. Client drafts, appearance and connection history
remain device preferences rather than a second workspace truth source.

The UI retains the runtime guard through shutdown: first drain the HTTP
listener and accepted operations, then stop workspace-owned children, release
the HTTP executor and finally the data lock. A shutdown timeout retains the
lock until cleanup or process exit rather than releasing it while children run.
Closing sharing or navigating away from a session does not stop owner tasks.

### Validation boundary

Source contracts and isolated fixtures do not prove a released binary,
installed-app upgrade, physical Android device or every Agent/provider works.
Use [testing guidance](../guides/testing-artifacts.md) and
[ACP validation scope](../guides/acp-backends.md#真实连接结果与验收范围) for the
specific checks. Ordinary fixture tests do not require real model calls.

## Shared host and CLI lanes

`work-host` owns GUI-independent startup, HTTP configuration and ordered cleanup.
`work-cli serve` and `work-ui --headless` call the same foreground host, lock and
workspace factory. The CLI binary has no Dioxus/WebView dependency. Headless
requires an explicit nonempty HTTP password; HTTP startup failure is fatal.
Ctrl+C, Unix SIGTERM, or explicitly enabled stdin EOF supervise shutdown;
there is no remote shutdown API. System-service installation is not implemented.

`work-ui --connect ORIGIN` (or explicit remote `RPI_WORK_NODE_URL`) starts only a
client. It never acquires the local data lock or silently creates an owner after
connection failure. Default owning GUI mode still uses its in-process facade.

`work-cli` now defaults to the existing node's authenticated HTTP/WebSocket entry at
`http://127.0.0.1:13031/`. `--url`/`--node` override `RPI_WORK_URL`; the client's
`RPI_WORK_PASSWORD` or bounded `--password-stdin` authenticates each invocation.
No plaintext credential store or cross-command login persistence is introduced.
All new operations use typed client methods; acceptance is not settlement and
write timeouts have unknown outcomes, without automatic replay. Watch/client
exit detaches rather than stopping owner tasks. See the [headless guide](../guides/headless-node.md).

```text
work-cli --help
work-cli auth
work-cli health
work-cli projects
work-cli sessions
work-cli session-create PROJECT_ID
work-cli prompt SESSION_ID "hello"
work-cli watch SESSION_ID
work-cli abort SESSION_ID
```

Direct RPI RPC/ACP diagnostics remain explicitly opt-in; these start their own
Agent and are not the unified node-client lane:

```text
RPI_WORK_TRANSPORT=rpc work-cli agent-info
RPI_WORK_TRANSPORT=rpc work-cli state
RPI_WORK_TRANSPORT=rpc work-cli messages [LIMIT] [BEFORE_ID]
RPI_WORK_TRANSPORT=rpc work-cli history [LIMIT] [AFTER_SEQ] [BEFORE_SEQ]
RPI_WORK_TRANSPORT=rpc work-cli queue [--all]
RPI_WORK_TRANSPORT=rpc work-cli models
RPI_WORK_TRANSPORT=rpc work-cli thinking-levels
RPI_WORK_TRANSPORT=rpc work-cli tools
RPI_WORK_TRANSPORT=rpc work-cli commands
RPI_WORK_TRANSPORT=rpc work-cli auth-status
RPI_WORK_TRANSPORT=acp work-cli load-session SESSION_ID
RPI_WORK_TRANSPORT=acp work-cli resume-session SESSION_ID
RPI_WORK_TRANSPORT=rpc work-cli set-config thinking '"high"'
RPI_WORK_TRANSPORT=rpc work-cli prompt "hello"
RPI_WORK_TRANSPORT=acp work-cli prompt "hello"
RPI_WORK_TRANSPORT=acp work-cli prompt --allow "hello"
```

The native CLI cancels ACP permission requests by default. With the explicit
`prompt --allow` opt-in, `PendingDialog::acp_permission_policy` selects an actual
advertised `allow_once` or `allow_always` option and uses its exact `optionId`,
not a hard-coded `"allow"` ID. If no allow option exists it cancels; malformed
or duplicate IDs fail validation. Prompt execution and native event consumption
are concurrent, so permission requests can be answered before the ACP prompt
response arrives.

## Native lifecycle and failure semantics

RPC and ACP retain terminal process state separately from the event broadcast
channel. EOF, fatal protocol input, or explicit stop releases pending requests;
existing subscribers drain buffered events, receive a terminal error, and end.
Subscriptions and requests created after termination fail promptly instead of
waiting on an already-drained response map. ACP closes outstanding permission
cards before publishing termination and does not synthesize a successful
`agent_settled` event for a crash.

Frame writes, including stdin-lock acquisition and flush, share the request's
30-second deadline with response waiting and can be interrupted by terminal
state. A failed or partial write invalidates the transport; later frames are
not allowed to continue on that stream. ACP tracks permission answers in flight
without holding the permission-state lock during I/O, so EOF can close cards
and release waiters even under backpressure. RPC gives graceful stop at most
250 ms before killing/reaping an unresponsive child. Repeated stop is safe.

A request cancellation before its frame is fully flushed invalidates the
transport; pending requests are failed synchronously and the child is killed
and reaped when a runtime is available. The incomplete-frame guard publishes
termination **before releasing stdin**, and queued writers recheck termination
after acquiring stdin. Cancellation after successful flush removes only that
request's pending sender: it does not abort the remote operation or poison the
transport. Late responses to that cancelled wait are ignored.

If an in-flight ACP permission-answer future is dropped, its cancellation guard
synchronously closes pending cards and fails requests, then initiates child
termination. The answer may have left a partial frame, so resetting the busy
flag and retrying on the same stream would be unsafe. A backpressure fixture
verifies this fail-closed path.

Cloned native clients share a public-handle lifetime separate from the stdout
reader's ownership. Dropping a non-last clone keeps the agent usable; dropping
the last handle invalidates the transport and initiates child cleanup even if
subscriptions remain. The lifetime guard is installed before the handshake
await, so failed or cancelled initialization also initiates cleanup. Outside a
runtime, cleanup uses immediate `start_kill` when the child lock is available;
async reaping requires a runtime and `kill_on_drop` remains the final fallback.

Timeout-bounded fixture tests cover abrupt exit, pending prompts/permissions,
unresponsive stop, late requests/subscriptions, last-clone drop, request and
permission cancellation under backpressure, and no successful settlement on
crash. Unit tests check immediate pending removal, child termination/reaping,
and stdin-unlock ordering with a queued writer. These guarantees do not imply
automatic native reconnection, history replay, or cross-process session
persistence.

## UI compatibility

The UI displays workspace protocol/channel status, while session execution and
configuration are gated by exact `SessionAccess` / `SessionConfiguration`
observations. `WorkspaceClient` routes local access to the same node owner via
`LocalWorkspace`, and remote access to `WorkClient`, without a UI-native
execution branch. Workspace capabilities include local Rust resources;
they must not be read as claims that the child RPC protocol advertises
project/file/Git commands. Unknown access is fail-closed and unsupported
operations return explicit errors.

`WorkClient` remains the stable HTTP API. `AgentClient::from_transport`,
`spawn_rpc`, and `spawn_acp` remain adapter entry points for native consumers
and CLI use, not alternative default desktop workspace backends.
