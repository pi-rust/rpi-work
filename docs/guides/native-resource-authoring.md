# Native resources and durable authoring

This guide describes the **formal Work implementation**, not the inert design preview. An agent is an ordinary RPI session selecting native Package/Skills/extensions/tools. There is no separate AgentProfile, daemon, build engine, or browser-owned execution configuration.

## Ownership and dependency

`work-core::api::NodeApi` → existing `work-node-runtime::workspace::LocalWorkspace` → reviewed `rpi-extcore`/native Service. Local clients call the same owner directly; authenticated HTTP is an adapter. NodeApi remains object-safe and non-Send. Node authorizes projects, local source roots and opaque references and stores authoring/provenance records. Native/core alone own installation, host lifecycle, exact configuration, revision CAS, operation recovery, history, admission and run settlement.

The current `rpi-extcore =0.2.11` dependency is pinned to `https://github.com/pi-rust/rpi-package`, revision `1e3c9551a054fe3bb6f2ef905429268bc4a7e5bd`. Cargo.lock records the same immutable Git source for the related core crates. No adjacent checkout/path fallback or machine-global Cargo override is required.

## Formal entry points

- **Resources → Native resources**: catalog and native inspect/install/update/reinstall/remove/recover, project/global scope. The [curated Package discovery panel](curated-packages.md) adds bundled data-only Package/Skill preview, local JSON download, verified node acquisition and explicit native installation; it is not an online market or arbitrary-URL importer. Inert design resources remain below, explicitly distinct.
- **Conversation → Resources**: main Package, explicit persistent instruction mappings, independent Skills/extensions, tools default/only/none, supplementary text; submit revision-CAS apply, reread, retry original ID, or explicitly supersede a safely blocked intent.
- **Meta Agent → Node authoring**: create Package/Skill/extension source draft, save revision/digest, static validation, create immutable real test session, execute real test, inspect settlement evidence, publish an immutable local version. The collapsed historical designer is memory-only and inert; it is not native testing or installation.
- **Developer conversation** starts an ordinary exact session using the shipped developer Package. Its native write/edit tools operate on the same displayed draft source files.

Normal session creation installs/selects the real shipped base Package before the first model request. Developer creation selects the shipped developer Package. Shipped resource-only source is compiled from `crates/node/work-node-runtime/assets/packages/{base,developer}` (the root `assets/packages` tree currently contains the same six source files) and prepared with immutable content/provenance checks under node-owned data. Installation uses native management and respects coarse in-use leases. It never silently replaces a user-owned installation. Existing unconfigured sessions remain explicitly legacy; an old native host must reject new exact creation before any legacy/wider-default execution. ACP customization and unsafe ACP-authoring admission are Unsupported, not simulated successes.

Catalog, editor selection, historical configuration observation, fresh actual loaded inventory, model-effective tools and execution readiness are separate facts. Verification compares **typed native binding and actual host generation**: native `operationId:null` and an omitted optional serialized field are equivalent, but old-generation observations are not. Readiness may be unknown; no persisted configuration alone proves executability.

Supplementary text is bounded to native's 64 KiB and stored in immutable node-owned UTF-8 files by content identity. Opaque installed references cannot select arbitrary instruction/DLL paths. Main Package instruction mappings must be canonical installed Package files. Skill references reverse-map through native `selectable.skills` entries (the SKILL.md), not directory paths. Omitted native Package scope means project.

## Applying and releasing

Exact resource changes use only typed native apply. The legacy per-tool `set_active_tools` setter is not exposed for exact sessions. A request has a stable operation ID, expected revision and immutable target; Work retains its authorized original request/resolved native intent so reconnect/restart retries can use the original ID without requiring the original installation source to exist. Reusing an ID for a different target conflicts. Work does not implement a second desired-configuration authority.

Busy/Ask/queued/unsettled work refuses apply/release. Global **release idle hosts** explicitly visits all already-owned Services, including the control Service, and returns released and blocked facts separately: cross-Service release is not falsely represented as atomic. It retains ID/history/configuration and never manipulates PID/lock files, external owners, or user services. A partial release requires reread and an explicit retry. Core captures latest owned idle model/provider/thinking on release/reopen; crash/raw external-stop limits remain core limits.

Closed/blocked sessions have a nonexecuting native saved-history fallback in the normal conversation view, with cursor pagination and blocked operation identity. An online, freshly observed pending/blocked operation has a separate explicit recovery lane: retry the original ID, or correct a safely blocked configuration. This requests core recovery, never fabricates a live idle projection or widens ordinary editing. Catalog refresh disables mutations until authoritative readback completes. This does not fabricate access, streams or a successful restore. Restore/apply must verify before input becomes executable.

## Durable source and evidence

Project source and metadata live under `<project>/.rpi/work-authoring`. Source is UTF-8, at most 64 files / 1 MiB aggregate, at most 256 KiB per file and bounded nesting/entries. Hidden source paths, traversal, symlink/junction/reparse ancestors and special files are rejected. Metadata is versioned and bounded to 8 MiB; capacity exhaustion refuses rather than evicting history.

All bounded source/store filesystem IO runs inside a **retained blocking worker**. The worker owns the cloned owner, serial authoring admission, lifecycle lease, and actual core project fence through actual exit, even if the UI/HTTP caller disconnects. Native side effects still run through the existing Service. Before entering native installation/creation, the source fence is released to avoid recursively acquiring core lifecycle. Publication obtains its catalog observation **before** acquiring the source fence; snapshot verification/copy remains fenced. Existing project ACP owners fail closed because their filesystem authority cannot be claimed as fenced.

Save checks revision/digest and reads actual files under admission, preserves old/new bytes, journals before replacing source, then atomically replaces metadata. External/model edits increase saved revision/digest and invalidate current validation/test eligibility; old test/release history stays immutable. Save conflicts/failures never clear the UI buffer. Refresh is explicit. Switching resource targets remounts session/project/generation-bound controls; old asynchronous results cannot be applied as another session's selection.

Static validation does not execute compilers, scripts, or DLLs, and is not an execution pass. Package and Skill templates are native source layouts, not legacy Work design JSON. Extension text remains explicitly unbuilt/ABI-unverified; Work does not offer arbitrary build argv or claim a source-only extension is executable. A separately authorized compatible native artifact may be managed through the native resource manager.

A test fixes the exact saved revision/digest into node-owned immutable SOURCE, installs it through native, creates a real exact session, records native readback, then separately executes an explicit real prompt. A durable test journal records intent before native side effects and installation/session receipts before later awaits can fail. Incomplete records appear as diagnostics, preserving recoverable session/snapshot identity rather than silently orphaning a created host. The retained worker finishes recording even when the caller drops.

A passed test requires `RunTracker`'s owned `Completed` settlement and unchanged binding/generation. Arbitrary assistant rows, schema validation and configuration readback do not pass. `PromptRun` admission is dropped after settlement before reacquiring the project fence. Native-body binding is used consistently in raw evidence. Settlement evidence is saved before source-fence reacquisition, so a racing new prompt cannot erase the sole observed result. A restarted session without usable proof needs fresh execution, not transcript inference.

Standalone Skill installation takes the snapshot **directory**, not its SKILL.md file. Native metadata requires the frontmatter name to equal the installed directory name; authoring uses that canonical name, never rewrites a tested snapshot to fit a random test ID. This means independent versions of the same standalone Skill name cannot all be installed in one native scope at once. A conflicting installed identity is refused: explicitly release idle hosts and remove/reinstall through the resource manager when changing the installed version; old SOURCE snapshots remain unchanged. Reuse the published Skill under the same canonical name. Native may omit Skill descriptions from model input when tools none prevents reading them; the acceptance test uses an explicit read-only tool policy for Skill model-input proof, independently of its tools-none Package proof.

Publish requires the exact current source revision/digest, immutable snapshot bytes, successful execution evidence and tested installed identity. It copies immutable source into a new node-owned local artifact/version and records it. It does not upload publicly, overwrite a version, mutate old tests or change a running session. Install/select the local artifact explicitly through the same resource page to reuse it in another ordinary session; tools none results in zero model tool schemas.

## Failure and recovery limits

- A failed/corrupt legacy localStorage read retains original bytes and displays an error. All historical autosave/write effects were removed. Explicit import preserves exact original inert bytes as a reference in a new canonical native draft; it never turns design JSON into execution authority.
- Corrupt/unsupported node metadata fails closed without resetting it. There is no automatic destructive repair.
- An uncertain save retains `pending.json`, `recovery/old`, proposed/replaced source, and metadata evidence. Further authoring writes refuse. Keep unsaved UI text; inspect/archive the entire authoring directory and reconcile actual source/metadata explicitly while project work is idle. Do not delete the journal or replay a save merely because the response timed out. Automated journal repair is intentionally not provided.
- Incomplete tests retain `test-journal/<id>/{intent,install-receipt,session,completed}.json` as available. An intent without a session receipt means **unknown**, not “no session”; inspect the core catalog and do not automatically recreate it.
- Immutable publication uses synced same-directory staging plus a no-replace hard link; unsupported filesystems fail instead of exposing a partially written final identity. Original and uncertain staging bytes are retained. Metadata replacement failures may occur on Windows if another process refuses delete sharing. Only OS codes 5/32/33 receive at most ten narrowly bounded same-publication attempts (5.11 seconds total wait on the blocking worker), checking unchanged old destination and candidate bytes before every attempt. A changed identity or exhausted retry reports failure, retains candidate/pending/recovery bytes, and never unlinks the destination or writes a final path directly. Owner diagnostics log operation/kind/raw OS code without source contents or credentials.
- The fence coordinates owned native work, not malicious/noncooperative external filesystem writers. Ancestor checks and final source digest checks reduce accidental races, but Work does not claim OS sandbox/openat-style race immunity or durability against filesystem/hardware failure beyond successful sync/rename semantics.

## Reproduction and evidence

```text
cargo build --locked -p work-client --bins
cargo test --locked -p work-core -p work-node -p work-client -p work-server
```

For the existing server ACP test when Cargo's target/build directories differ, explicitly set `WORK_SESSION_ACCESS_ACP_FIXTURE` to the built owned `debug/acp-fixture.exe`. These are credential-free test fixtures, not installed external ACP agents.

Real opt-in native acceptance:

```text
cargo build --locked -p work-server --example resource_workflow
# Explicit absolute RPI_RESOURCE_NATIVE_BIN, WORK_CDE_HOST, WORK_CDE_BUILD required
python tests/e2e/native-resource-workflow.py
```

The acceptance host verifies the authorized native binary SHA-256 and requires isolated agent/session/project/provider roots. It binds ephemeral loopback HTTP; the model fixture is also loopback-only, uses no credentials and records actual native model requests. It exercises shipped first-turn load, developer native write of managed draft files, stale CAS safety, native in-use refusal/release, real immutable test and settlement-gated publication, published reuse with tools none, independent same-project sessions, idempotent/stale apply and same-ID owner restart/history/drafts. An optional `WORK_CDE_BROWSER_SCRIPT` can use its exported context against the actual formal bundle, never a fake native transport.

Formal build: `tools/dev/build-ui-web.ps1 -DebugBuild -OutputRoot <owned-build>/ui`; generated same-origin assets are `<owned-build>/ui/parity-web-server`. OutputRoot keeps generated bytes outside the source checkout; the existing default paths remain unchanged. Android checks use the existing shared NDK wrapper (`scripts/ci/check-android.ps1`) for both client and mobile UI. Browser screenshots/logs/results are local evidence, not production source or a substitute for real native tests.
