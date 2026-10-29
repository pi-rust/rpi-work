# RPI Work transport architecture

`work-client::WorkspaceClient` is the desktop UI's local/remote workspace
facade. `Local` uses the published pure-Rust `rpi-extcore =0.1.5` service
in-process; `Remote(WorkClient)` uses authenticated HTTP/SSE. The separate
`AgentTransport` / `AgentClient` adapters support native RPC and explicit CLI
HTTP/ACP use; they are not a desktop split-lane selector.

```text
Desktop UI → WorkspaceClient::Local → LocalWorkspace → rpi-extcore (in-process)
                                      ├── local Rust projects/catalog/files/Git
                                      └── Agent execution/control → rpi --mode rpc (stdio)
Web / Android / explicit remote desktop → WorkspaceClient::Remote
                                         → WorkClient → authenticated HTTP/SSE node
CLI → AgentClient → RPI RPC (default), or explicit HTTP/SSE / ACP
```

This describes current source, not a newly shipped binary or an installed-app
upgrade.

## Protocol boundaries

`work-core::AgentDescriptor` identifies the protocol and transport and carries
an operation-level capability set. ACP and native RPC support prompts and
streaming, but do not automatically provide HTTP workspace APIs for projects,
files, or Git diffs. Native RPC does expose its own session-scoped history,
messages, queue, model/tool configuration, and provider-auth commands; these
are separate from HTTP workspace resources and are advertised only when the
ready frame lists the corresponding command.

Callers use `AgentDescriptor::supports` or `AgentClient::require` before
presenting those operations. A transport returns `ClientError::Unsupported`
when a command is outside its negotiated surface; it must not fabricate a
successful response.

Desktop startup uses `work-node::local::LocalRuntime` to resolve `rpi` and
hold the existing data-directory lock. It then creates
`WorkspaceClient::local_rpc`; it does not spawn or require `rpi-http`, open a
listener, generate a node password, or authenticate over loopback. The retained
`desktop::DesktopNode` compatibility implementation is not the default UI
startup path. Local workspace resources are Rust service operations, not
invented project/file/Git commands on the RPI RPC wire.

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

`AcpClient` starts an ACP stdio executable and performs ACP v1
`initialize`/`session/new` negotiation. It supports text prompts, cancellation,
streaming `session/update` notifications, permission requests, session config
options, and explicit JSON-RPC responses to agent-initiated requests. The
client intentionally negotiates no ACP filesystem or terminal client capability
because those request handlers are not implemented in this crate;
agent-initiated permission requests remain explicitly handled through the
permission-response facade. ACP is a client adapter here; the sibling `rpi-acp`
package remains an ACP server and is not imported as an implementation
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
inventing choices; a configOptions-driven ACP UI is not part of the current
desktop workspace.

`AgentClient::supports_command` adds operation-level gating to facade reads and
controls. RPC checks the actual ready command list; a broad `history=true` does
not enable an omitted `compact` or `resume_pending`. ACP allows only the
implemented prompt/cancel command mappings; config uses its dedicated setter.
The adapters include command checks for discovery and context controls. These
checks describe the adapter surface, not a generic ACP desktop settings UI;
the default desktop workspace uses `LocalWorkspace`, not an ACP session lane.

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

## Desktop native workspace

The default desktop workspace is fully native, not RPC sessions plus HTTP
workspace resources. `LocalWorkspace` owns the app catalog and per-project
`rpi-extcore::service::Service` instances. Projects, folders, files and Git are
local Rust operations; all Agent execution/control uses `rpi --mode rpc`
stdio children owned by the core service. Workspace session lists, snapshots
and subscriptions are supplied by that service, not by inventing RPC wire
methods or assuming the lower-level `RpiRpcClient` supports cross-process
`session/load` or `session/resume`.

Native folder selection uses the OS picker and accepts arbitrary paths the
current user can access. Creating a project uses the exact selected/entered
absolute path. Remote folder browsing and creation retain the node's root and
API restrictions; they do not gain access to the desktop filesystem.

`rpi-work://local` is a stable UI identity, not a network endpoint. Web,
Android, and explicit remote connections require a separately reachable,
authenticated HTTP/SSE node. Sharing this computer by address/QR requires
starting such a node separately; desktop startup opens no port. There is no
implicit HTTP fallback or desktop `RPI_WORK_TRANSPORT=http` / `acp` selector.
Those environment choices remain explicit CLI alternatives.

### Local data and migration

Let `<data>` be the existing platform `rpi-work/node` directory, or
`RPI_WORK_DATA_DIR`. The same `<data>/node.lock` protects ownership. The native
catalog is `<data>/rpc/projects.json`; each project registry is
`<data>/rpc/projects/<id>/sessions.json`. The lazily created model/auth control
context is separate from user projects under `<data>/rpc/control`.

`local_workspace/migration.rs` imports accessible project aliases from
`<data>/projects.json` and `<data>/node/projects.json`, deduplicated by canonical
project identity. It imports missing entries and preferences from
`<data>/sessions.json` and `<data>/node/sessions.json` only when `projectId`
matches an imported/registered project. Existing target entries are retained;
unavailable paths and all legacy source files are preserved. No transcript is
rewritten, and this is not global native-history discovery or a scan of all
user session roots.

The UI retains the runtime guard through shutdown: core-owned children are
stopped before releasing the data lock. A shutdown timeout retains the lock
until cleanup or process exit rather than releasing it while children run.

### Validation scope

Reported checks for this source change: `work-client`, `work-node`, and
`work-ui` tests; desktop and WASM checks; and the explicitly run ignored
`local_workspace_smoke` against rpi 0.11.0 without model calls. Native GUI
folder-picker interaction and real model chat have not been manually tested.
No binary release or installed-app upgrade has occurred.

## Native CLI lane

`work-cli` uses native RPI RPC by default and exposes HTTP/SSE and ACP as
explicit alternatives:

```text
work-cli agent-info
work-cli state
work-cli messages [LIMIT] [BEFORE_ID]
work-cli history [LIMIT] [AFTER_SEQ] [BEFORE_SEQ]
work-cli queue [--all]
work-cli models
work-cli thinking-levels
work-cli tools
work-cli commands
work-cli auth-status
RPI_WORK_TRANSPORT=http work-cli health
RPI_WORK_TRANSPORT=http work-cli projects
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

The UI stores the workspace descriptor, displays protocol/channel status, and
gates operations by the exposed capabilities. `WorkspaceClient` routes the
whole local workspace to `LocalWorkspace`, and remote workspaces to
`WorkClient`. Native workspace capabilities include local Rust resources;
they must not be read as claims that the child RPC protocol advertises
project/file/Git commands. Unsupported operations return explicit errors.

`WorkClient` remains the stable HTTP API. `AgentClient::from_transport`,
`spawn_rpc`, and `spawn_acp` remain adapter entry points for native consumers
and CLI use, not alternative default desktop workspace backends.
