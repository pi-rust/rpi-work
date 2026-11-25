const STORAGE_KEY = 'rpi-work.conversation-reading.v1'
const MAX_SCOPES = 128
const MAX_SCOPE_LENGTH = 2048
const MAX_ANCHOR_LENGTH = 1024
const MAX_STORAGE_LENGTH = 512 * 1024
const WRITE_INTERVAL_MS = 200

export type ReadingCheckpoint = {
  top: number
  atBottom: boolean
  anchor: { key: string; top: number } | null
}

function read() {
  const checkpoints = new Map<string, ReadingCheckpoint>()
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw || raw.length > MAX_STORAGE_LENGTH) return checkpoints
    const entries: unknown = JSON.parse(raw)
    if (!Array.isArray(entries)) return checkpoints
    for (const entry of entries.slice(-MAX_SCOPES)) {
      if (!Array.isArray(entry) || typeof entry[0] !== 'string' || entry[0].length > MAX_SCOPE_LENGTH) continue
      const [scope, value] = entry as [string, Partial<ReadingCheckpoint> | null]
      if (!value || typeof value.top !== 'number' || !Number.isFinite(value.top) || value.top < 0 || typeof value.atBottom !== 'boolean') continue
      const anchor = value.anchor
      checkpoints.set(scope, {
        top: value.top,
        atBottom: value.atBottom,
        anchor: !value.atBottom && anchor && typeof anchor.key === 'string' && anchor.key.length > 0 &&
          anchor.key.length <= MAX_ANCHOR_LENGTH && Number.isFinite(anchor.top) ? { key: anchor.key, top: anchor.top } : null,
      })
    }
  } catch { /* Storage is optional. */ }
  return checkpoints
}

/** Local display positions only. The history, messages and unread state stay outside storage. */
export function createReadingCheckpoints() {
  const pending = new Map<string, ReadingCheckpoint>()
  let timer: ReturnType<typeof setTimeout> | undefined

  const flush = () => {
    clearTimeout(timer)
    timer = undefined
    if (!pending.size) return
    const saved = read()
    for (const [scope, checkpoint] of pending) {
      saved.delete(scope)
      saved.set(scope, checkpoint)
    }
    pending.clear()
    try {
      const entries = [...saved].slice(-MAX_SCOPES).map(entry => JSON.stringify(entry))
      let size = 2
      let start = entries.length
      while (start > 0 && size + entries[start - 1].length + 1 <= MAX_STORAGE_LENGTH) {
        size += entries[--start].length + 1
      }
      localStorage.setItem(STORAGE_KEY, `[${entries.slice(start).join(',')}]`)
    } catch { /* In-memory positions are still usable. */ }
  }

  return {
    get: (scope: string) => pending.get(scope) ?? read().get(scope),
    save: (scope: string, checkpoint: ReadingCheckpoint) => {
      if (scope.length > MAX_SCOPE_LENGTH) return
      const anchor = checkpoint.anchor
      pending.set(scope, { ...checkpoint, anchor: anchor && anchor.key.length <= MAX_ANCHOR_LENGTH ? anchor : null })
      if (pending.size > MAX_SCOPES) pending.delete(pending.keys().next().value!)
      timer ??= setTimeout(flush, WRITE_INTERVAL_MS)
    },
    flush,
  }
}
