// Deterministic test of the actual shared refresh scheduler, without a browser.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const sent = [], timers = new Map(), listeners = new Map();
let serial = 0;
const document = {
  hidden: false,
  addEventListener: (name, fn) => listeners.set(name, fn),
  removeEventListener: name => listeners.delete(name),
};
const window = {
  addEventListener: (name, fn) => listeners.set(name, fn),
  removeEventListener: name => listeners.delete(name),
};
vm.runInNewContext(fs.readFileSync('apps/work-ui/src/app/workspace_sync.js', 'utf8'), {
  id: 1, window, document,
  dioxus: { send: value => sent.push(value) },
  clearTimeout: id => timers.delete(id),
  setTimeout: (fn, delay) => { const id = ++serial; timers.set(id, { fn, delay }); return id; },
});
const state = window.__rpiWorkspaceSync.get(1);
assert.deepEqual(sent, [false]);
state.request(1); state.request(1);
assert.deepEqual(sent, [false], 'manual requests never overlap an automatic in-flight refresh');
state.complete();
assert.deepEqual(sent, [false, true], 'a new request gets a fresh follow-up snapshot');
state.request(1); state.complete();
assert.deepEqual(sent, [false, true]);
assert.equal(timers.size, 1);
assert.equal([...timers.values()][0].delay, 3000);
state.request(2);
assert.deepEqual(sent, [false, true, true]);
assert.equal(timers.size, 0, 'manual click bypasses the polling delay');
state.request(3); state.request(4); state.complete();
assert.deepEqual(sent, [false, true, true, true], 'several requests coalesce into one follow-up');
state.complete();
document.hidden = true; listeners.get('visibilitychange')();
state.request(5);
assert.equal(sent.length, 4);
document.hidden = false; listeners.get('visibilitychange')();
assert.deepEqual(sent, [false, true, true, true, true], 'manual intent survives a hidden window');
state.complete(); state.dispose(); state.request(6);
assert.equal(sent.length, 5);
assert.equal(timers.size, 0); assert.equal(listeners.size, 0);
assert.equal(window.__rpiWorkspaceSync.size, 0);
console.log('PASS manual refresh scheduler: immediate pull, no overlap, deduplication, queued fresh snapshot, visibility and disposal');
