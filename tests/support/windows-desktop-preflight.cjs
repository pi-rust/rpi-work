// Read-only execution-context probe. Never creates a window, switches sessions,
// impersonates a user, or changes WebView2/system policy.
'use strict';
const { spawnSync } = require('node:child_process');
function assessDesktopContext(context) {
  if (!context || !Number.isInteger(context.sessionId) || context.sessionId < 0
    || typeof context.userInteractive !== 'boolean') {
    return { supportedPlatform: true, probeSucceeded: false, interactiveDesktop: false,
      reason: 'desktop-context-unconfirmed' };
  }
  const interactiveDesktop = context.sessionId !== 0 && context.userInteractive;
  return { supportedPlatform: true, probeSucceeded: true,
    sessionId: context.sessionId, userInteractive: context.userInteractive, interactiveDesktop,
    reason: interactiveDesktop ? null : 'interactive-windows-session-required' };
}
function desktopPreflight() {
  if (process.platform !== 'win32') {
    return { supportedPlatform: false, probeSucceeded: true, interactiveDesktop: false,
      reason: 'windows-required' };
  }
  const script = `$ErrorActionPreference='Stop'; $p=Get-Process -Id ${process.pid}; @{sessionId=$p.SessionId; userInteractive=[Environment]::UserInteractive} | ConvertTo-Json -Compress`;
  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script],
    { windowsHide: true, timeout: 10000, encoding: 'utf8' });
  try {
    if (result.status !== 0) throw new Error('ContextProbeFailed');
    return assessDesktopContext(JSON.parse(result.stdout));
  } catch {
    return { supportedPlatform: true, probeSucceeded: false, interactiveDesktop: false,
      reason: 'desktop-context-unconfirmed' };
  }
}
module.exports = { desktopPreflight, assessDesktopContext };
if (require.main === module) {
  const result = desktopPreflight();
  console.log(JSON.stringify(result));
  process.exitCode = result.interactiveDesktop ? 0 : 2;
}
