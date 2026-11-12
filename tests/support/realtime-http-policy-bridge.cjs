// Synthetic fixture adapter for suites whose fault policies use Playwright HTTP
// routes. The production client still uses real Query/Reserve/Execute frames and
// validates native realtime results; only the fixture's command implementation
// is delegated to its existing HTTP handler. Not a production HTTP fallback or
// a full server-side realtime implementation/conformance test.
async function bridgeRealtimePolicies(page, allowedOrigins, shouldDisconnect = () => false) {
  await page.routeWebSocket('**/api/v1/realtime', socket => {
    const url = new URL(socket.url());
    url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
    if (!allowedOrigins().includes(url.origin)) throw new Error(`Unexpected fixture socket: ${url.origin}`);
    if (shouldDisconnect()) return socket.close({ code: 1011, reason: 'fixture disconnected' });
    const server = socket.connectToServer();
    const pending = new Map(), sessions = new Map();
    server.onMessage(message => {
      const frame = JSON.parse(String(message));
      if (frame.type === 'reserved') {
        sessions.set(frame.operation.operationId, pending.get(frame.requestId));
        pending.delete(frame.requestId);
      }
      socket.send(message);
    });
    socket.onMessage(async message => {
      if (shouldDisconnect()) return socket.close({ code: 1011, reason: 'fixture disconnected' });
      const frame = JSON.parse(String(message));
      if (frame.type === 'reserve') pending.set(frame.requestId, frame.sessionId);
      if (!['query', 'execute'].includes(frame.type)) return server.send(message);
      const session = frame.type === 'query' ? frame.sessionId : sessions.get(frame.operation?.operationId);
      if (!session) throw new Error('Fixture bridge requires a reserved session');
      try {
        const value = await page.evaluate(async ({ endpoint, payload }) => {
          const response = await fetch(endpoint, { method: 'POST', credentials: 'include', headers: { 'content-type': 'application/json' }, body: JSON.stringify(payload) });
          return { ok: response.ok, status: response.status, data: await response.json() };
        }, { endpoint: `${url.origin}/api/v1/sessions/${encodeURIComponent(session)}/${Object.hasOwn(frame.payload, 'configId') ? 'configuration' : 'commands'}`, payload: frame.payload });
        if (!value.ok) {
          if (frame.type === 'query') throw new Error(`Fixture query policy returned ${value.status}`);
          // An explicit fixture HTTP failure maps to a native failed outcome,
          // not a transport disconnect (which would inject unrelated hydration).
          socket.send(JSON.stringify({ type: 'status', version: 1, requestId: frame.requestId, operation: frame.operation, status: { state: 'knownResult', outcome: { status: 'failed', error: { code: 'internal', message: JSON.stringify(value.data) } } } }));
          return;
        }
        socket.send(JSON.stringify(frame.type === 'query'
          ? { type: 'result', version: 1, requestId: frame.requestId, value: value.data }
          : { type: 'status', version: 1, requestId: frame.requestId, operation: frame.operation, status: { state: 'knownResult', outcome: { status: 'succeeded', value: value.data } } }));
      } catch (error) {
        // Closed contexts/timeouts are expected during teardown. Do not invent a
        // successful result or retry a potentially completed mutation.
        if (!page.isClosed()) socket.close({ code: 1011, reason: 'fixture policy bridge failed' });
      }
    });
  });
}
module.exports = { bridgeRealtimePolicies };
