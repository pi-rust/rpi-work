const STORAGE_KEY = ['rpi-work', 'directory-expansion', 'v1'].join('.')
const MAX_SCOPES = 128
const MAX_DIRECTORIES = 128
const MAX_PATH_LENGTH = 1024
const MAX_STORAGE_LENGTH = 512 * 1024

function readExpansion(): Map<string, string[]> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw || raw.length > MAX_STORAGE_LENGTH) return new Map()
    const records: unknown = JSON.parse(raw)
    if (!Array.isArray(records)) return new Map()
    const expansion = new Map<string, string[]>()
    for (const record of records.slice(-MAX_SCOPES)) {
      if (!Array.isArray(record) || typeof record[0] !== 'string' || !Array.isArray(record[1])) continue
      const paths = record[1].slice(0, MAX_DIRECTORIES).filter(isDirectoryPath)
      expansion.set(record[0], [...new Set<string>(paths)])
    }
    return expansion
  } catch { return new Map() }
}

function isDirectoryPath(value: unknown): value is string {
  return typeof value === 'string' && value.length <= MAX_PATH_LENGTH &&
    value.split('/').every(segment => segment && segment !== '.' && segment !== '..')
}

export function readDirectoryExpansion(scope: string): string[] {
  return readExpansion().get(scope) ?? []
}

export function saveDirectoryExpansion(scope: string, paths: string[]) {
  const expansion = readExpansion()
  expansion.delete(scope)
  const savedPaths = paths.filter(isDirectoryPath).slice(0, MAX_DIRECTORIES)
  if (savedPaths.length) expansion.set(scope, savedPaths)
  try {
    const records = [...expansion].slice(-MAX_SCOPES).map(record => JSON.stringify(record))
      .filter(record => record.length + 2 <= MAX_STORAGE_LENGTH)
    let size = 2
    let first = records.length
    while (first > 0 && size + records[first - 1].length + 1 <= MAX_STORAGE_LENGTH) {
      size += records[--first].length + 1
    }
    localStorage.setItem(STORAGE_KEY, `[${records.slice(first).join(',')}]`)
  } catch { /* The App keeps the local expansion intent in memory. */ }
}
