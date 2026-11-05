// Small HTTP-only contract smoke. No browser, Rust build, agent, model or user session.
const assert = require('node:assert/strict');
const { startFixture, fixtureCommand } = require('../fixtures/ui-fixture.cjs');
const capabilityKeys = ['sessions','sessionLoad','sessionResume','prompt','streaming','abort','dialogs','projects','files','gitDiff','models','thinkingLevels','tools','providerManagement','queue','history'].sort();
async function check(options) {
  const fixture = await startFixture(0, options);
  const call = async (path, body, cookie) => {
    const response = await fetch(fixture.url + 'api/v1/' + path, {
      method: body === undefined ? 'GET' : 'POST',
      headers: { 'content-type': 'application/json', ...(cookie ? { cookie } : {}) },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    return { status: response.status, cache: response.headers.get('cache-control'), data: await response.json() };
  };
  const pending = async (id = 'dark') => (await call(`sessions/${id}/snapshot`)).data.pendingDialogs;
  try {
    const access = await call('sessions/dark/access');
    assert.equal(access.status, 200);
    assert.equal(access.cache, 'no-store');
    assert.deepEqual(Object.keys(access.data).sort(), ['descriptor','supportedCommands','snapshotLimit','dialogReply'].sort());
    assert.deepEqual(Object.keys(access.data.descriptor.identity).sort(), ['protocol','version','transport','agentName'].sort());
    assert.equal(access.data.descriptor.identity.protocol, 'rpi');
    assert.deepEqual(Object.keys(access.data.descriptor.capabilities).sort(), capabilityKeys);
    assert(Object.values(access.data.descriptor.capabilities).every(value => typeof value === 'boolean'));
    assert.equal(access.data.snapshotLimit, true, 'RPI service snapshot limit is independent of UI get_messages');
    assert.equal(access.data.dialogReply, true);
    assert.equal(access.data.supportedCommands.includes('get_messages'), !!options.pagedHistory && !options.noMessagePaging);
    for (const command of ['prompt','abort','get_state','get_queue','ui_dialog_response','resume_pending']) assert(access.data.supportedCommands.includes(command));
    if (access.data.supportedCommands.includes('get_messages')) {
      fixture.setMessages(Array.from({ length: 5 }, (_, i) => ({ entryId: `m-${i}`, role: 'user', content: `fixture ${i}` })));
      const page = await call('sessions/dark/commands', { type: 'get_messages', limit: 2, beforeId: 'm-4' });
      assert.equal(page.data.success, true);
      assert.deepEqual(page.data.data.messages.map(m => m.entryId), ['m-2','m-3']);
    } else {
      assert.equal((await call('sessions/dark/commands', { type: 'get_messages' })).data.success, false);
    }
    // A per-session empty observation must not inherit the global command list.
    fixture.controls.sessionAccessById = { architecture: { supportedCommands: [], snapshotLimit: false, dialogReply: false } };
    const restricted = (await call('sessions/architecture/access')).data;
    assert.deepEqual(restricted.supportedCommands, []);
    assert.equal(restricted.snapshotLimit, false);
    assert.equal(restricted.dialogReply, false);
    assert((await call('capabilities')).data.supportedCommands.includes('prompt'));
    assert.equal((await call('sessions/dark/access')).data.supportedCommands.includes('prompt'), true);
    assert.equal((await call('sessions/architecture/commands', { type: 'get_state' })).data.success, false);
    fixture.emit({ type: 'ui_dialog_request', requestId: 'same-id', ui: { type: 'input' } }, 'architecture');
    fixture.emit({ type: 'ui_dialog_request', requestId: 'same-id', ui: { type: 'input' } });
    const answer = { requestId: 'same-id', answer: { kind: 'selector', values: ['Rust'] } };
    for (const invalid of [
      { requestId: 'same-id', answer: {}, cancelled: true },
      { requestId: 'same-id', cancelled: false },
      { requestId: 'same-id', responseId: 'raw-agent-id', answer: {} },
      { answer: {} },
    ]) assert.equal((await call('sessions/dark/dialogs', invalid)).status, 400);
    assert.equal((await pending()).length, 1);
    fixture.controls.failDialog = true;
    const rejected = await call('sessions/dark/dialogs', answer);
    assert.equal(rejected.status, 400);
    assert.match(rejected.data.error, /dialog response rejected/);
    assert.equal((await pending()).length, 1, 'failure retains pending card');
    fixture.controls.failDialog = false;
    const accepted = await call('sessions/dark/dialogs', answer);
    assert.equal(accepted.status, 200);
    assert.equal(accepted.cache, 'no-store');
    assert.equal(accepted.data.accepted, true);
    assert.deepEqual(await pending(), []);
    assert.equal((await pending('architecture')).length, 1, 'reply cannot close another session card');
    assert.equal((await call('sessions/architecture/dialogs', answer)).status, 501);
    fixture.emit({ type: 'ui_dialog_request', requestId: 'cancel-me', ui: { type: 'input' } });
    assert.equal((await call('sessions/dark/dialogs', { requestId: 'cancel-me', cancelled: true })).status, 200);
    assert.deepEqual(await pending(), []);
    // Legacy command and typed endpoint share exactly the failure/removal path.
    fixture.emit({ type: 'ui_dialog_request', requestId: 'legacy', ui: { type: 'input' } });
    fixture.controls.failDialog = true;
    assert.equal((await call('sessions/dark/commands', { type: 'ui_dialog_response', requestId: 'legacy', answer: null })).data.success, false);
    assert.equal((await pending()).length, 1);
    fixture.controls.failDialog = false;
    assert.equal((await call('sessions/dark/commands', { type: 'ui_dialog_response', requestId: 'legacy', cancelled: true })).data.success, true);
    assert.deepEqual(await pending(), []);
    const recorded = fixture.requests.filter(r => fixtureCommand(r) === 'ui_dialog_response');
    assert(recorded.some(r => r.path.endsWith('/dialogs') && !Object.hasOwn(r.body, 'type')));
    assert.deepEqual(recorded.find(r => r.path.endsWith('/dialogs') && r.body.answer?.kind === 'selector').body, answer);
    assert.equal((await call('sessions/unknown/access')).status, 404);
    assert.equal((await call('sessions/unknown/dialogs', answer)).status, 404);
    assert.equal((await call('sessions/dark/dialogs')).status, 405);
    fixture.controls.accessStatus = 503;
    assert.equal((await call('sessions/dark/access')).status, 503);
  } finally { await fixture.close(); }
}
(async () => {
  for (const options of [{}, { pagedHistory: true }, { pagedHistory: true, noMessagePaging: true }, { sessionAccess: { capabilities: { files: false } } }]) await check(options);
  const fixture = await startFixture(0, { authRequired: true, sessionAccess: { supportedCommands: [], dialogReply: false } });
  try {
    for (const suffix of ['access','dialogs']) {
      const response = await fetch(fixture.url + `api/v1/sessions/dark/${suffix}`, suffix === 'dialogs' ? { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ requestId: 'x', cancelled: true }) } : {});
      assert.equal(response.status, 401);
    }
    const response = await fetch(fixture.url + 'api/v1/sessions/dark/access', { headers: { cookie: 'rpi_web_session=fixture-session-only' } });
    assert.equal(response.status, 200);
    assert.deepEqual((await response.json()).supportedCommands, []);
  } finally { await fixture.close(); }
  console.log('PASS isolated UI fixture SessionAccess and typed dialogs HTTP contract');
})().catch(error => { console.error(error); process.exitCode = 1; });
