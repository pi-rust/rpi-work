'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { assessDesktopContext } = require('../support/windows-desktop-preflight.cjs');

test('Session 0 cannot qualify as interactive GUI acceptance', () => {
  for (const userInteractive of [false, true]) {
    const result = assessDesktopContext({ sessionId: 0, userInteractive });
    assert.equal(result.interactiveDesktop, false);
    assert.equal(result.reason, 'interactive-windows-session-required');
  }
});

test('a positive session still requires an interactive window station', () => {
  assert.equal(assessDesktopContext({ sessionId: 1, userInteractive: false }).interactiveDesktop, false);
  const result = assessDesktopContext({ sessionId: 1, userInteractive: true });
  assert.equal(result.interactiveDesktop, true);
  assert.equal(result.reason, null); // Only preflight readiness, never UI success.
});

test('missing or invalid evidence fails closed', () => {
  for (const context of [null, [], {}, { sessionId: -1, userInteractive: true },
    { sessionId: '1', userInteractive: true }, { sessionId: 1, userInteractive: 'true' },
    { sessionId: 1.5, userInteractive: true }]) {
    const result = assessDesktopContext(context);
    assert.equal(result.probeSucceeded, false);
    assert.equal(result.interactiveDesktop, false);
    assert.equal(result.reason, 'desktop-context-unconfirmed');
  }
});

test('report keeps only explicit non-secret context fields', () => {
  const result = assessDesktopContext({ sessionId: 1, userInteractive: true,
    arbitraryPrivateDiagnostic: 'not-for-the-report' });
  assert.equal(JSON.stringify(result).includes('not-for-the-report'), false);
  assert.deepEqual(Object.keys(result).sort(),
    ['interactiveDesktop', 'probeSucceeded', 'reason', 'sessionId', 'supportedPlatform', 'userInteractive'].sort());
});
