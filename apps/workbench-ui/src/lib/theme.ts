/**
 * Theme bridge: `work-viewmodel::theme` tokens → shadcn CSS variables.
 *
 * The palette, radii and density all come from the Rust token contract
 * (`theme_contract::build_tokens`); this module only renames keys so the
 * component library can consume them. No colour is defined in TypeScript.
 */
import { useCallback, useEffect, useMemo, useState, type CSSProperties } from 'react'

import { themePresets, themeTokens } from '@/lib/viewmodel'

export type ThemePreset = { id: string; name: string; description: string }
export type ThemeChoice = { id: string; dark: boolean; compact: boolean }

const KEY = 'rpi-work-reui-theme-v1'
const DEFAULT: ThemeChoice = { id: 'codex', dark: false, compact: false }

/** Rust token key → shadcn CSS variable. */
const MAP: Record<string, string> = {
  bg: '--background',
  text: '--foreground',
  card: '--card',
  code: '--code-background',
  muted: '--muted-foreground',
  side: '--secondary',
  'accent-bg': '--accent',
  accent: '--primary',
  'on-accent': '--primary-foreground',
  line: '--border',
  danger: '--destructive',
}

function read(): ThemeChoice {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return DEFAULT
    const value = JSON.parse(raw) as Partial<ThemeChoice>
    return {
      id: typeof value.id === 'string' && value.id.length > 0 ? value.id : DEFAULT.id,
      dark: value.dark === true,
      compact: value.compact === true,
    }
  } catch {
    return DEFAULT
  }
}

function persist(choice: ThemeChoice) {
  try {
    localStorage.setItem(KEY, JSON.stringify(choice))
  } catch {
    // Blocked storage keeps the choice in memory only.
  }
}

/** Build the CSS variable set for one token map. */
export function themeStyle(tokens: Record<string, string>): CSSProperties {
  const style: Record<string, string> = {}
  for (const [token, variable] of Object.entries(MAP)) {
    const value = tokens[token]
    if (value) style[variable] = value
  }
  // Derived shadcn variables that reuse the same tokens.
  const alias: [string, string][] = [
    ['--card-foreground', 'text'],
    ['--popover', 'card'],
    ['--popover-foreground', 'text'],
    ['--secondary-foreground', 'text'],
    ['--accent-foreground', 'text'],
    ['--input', 'line'],
    ['--ring', 'accent'],
    ['--sidebar', 'side'],
    ['--sidebar-foreground', 'text'],
    ['--sidebar-primary', 'accent'],
    ['--sidebar-primary-foreground', 'on-accent'],
    ['--sidebar-accent', 'accent-bg'],
    ['--sidebar-accent-foreground', 'text'],
    ['--sidebar-border', 'line'],
    ['--sidebar-ring', 'accent'],
  ]
  for (const [variable, token] of alias) {
    const value = tokens[token]
    if (value) style[variable] = value
  }
  // Syntax colours for the ReUI `css-variables` shiki theme: the highlighter
  // emits `var(--code-token-*)`, so code follows the Rust palette too.
  const code: [string, string][] = [
    ['--code-foreground', 'body-text'],
    ['--code-token-constant', 'code-constant'],
    ['--code-token-string', 'code-string'],
    ['--code-token-comment', 'code-comment'],
    ['--code-token-keyword', 'code-keyword'],
    ['--code-token-parameter', 'text'],
    ['--code-token-function', 'text'],
    ['--code-token-string-expression', 'code-string'],
    ['--code-token-punctuation', 'muted'],
    ['--code-token-link', 'accent'],
  ]
  for (const [variable, token] of code) {
    const value = tokens[token]
    if (value) style[variable] = value
  }
  const radius = tokens['radius-lg']
  if (radius) style['--radius'] = radius
  // Density rides on the root font size: every rem-based Tailwind value scales.
  const factor = Number(tokens['density-factor'] ?? '1')
  style['fontSize'] = `calc(16px * ${Number.isFinite(factor) && factor > 0 ? factor : 1})`
  return style as CSSProperties
}

export function useTheme(ready: boolean) {
  const [choice, setChoice] = useState<ThemeChoice>(DEFAULT)
  const [presets, setPresets] = useState<ThemePreset[]>([])

  useEffect(() => {
    setChoice(read())
  }, [])

  useEffect(() => {
    if (!ready) return
    setPresets(themePresets())
  }, [ready])

  const tokens = useMemo(
    () => (ready ? themeTokens(choice.id, choice.dark, choice.compact) : {}),
    [ready, choice],
  )
  const style = useMemo(() => themeStyle(tokens), [tokens])

  const update = useCallback((next: ThemeChoice) => {
    setChoice(next)
    persist(next)
  }, [])

  return {
    choice,
    presets,
    style,
    tokens,
    setTheme: (id: string) => update({ ...choice, id }),
    setDark: (dark: boolean) => update({ ...choice, dark }),
    setCompact: (compact: boolean) => update({ ...choice, compact }),
  }
}
