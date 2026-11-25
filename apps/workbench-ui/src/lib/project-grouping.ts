import { useState } from 'react'

export type ProjectGrouping = 'node' | 'flat'
const KEY = 'rpi.work.project-grouping.v1'

export function useProjectGrouping() {
  const [grouping, setGrouping] = useState<ProjectGrouping>(() => {
    try { return localStorage.getItem(KEY) === 'flat' ? 'flat' : 'node' }
    catch { return 'node' }
  })
  const updateGrouping = (next: ProjectGrouping) => {
    setGrouping(next)
    try { localStorage.setItem(KEY, next) } catch { /* Keep the choice for the current page. */ }
  }
  return { grouping, updateGrouping }
}
