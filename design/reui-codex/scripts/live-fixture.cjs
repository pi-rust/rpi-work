// Start the repository's UI fixture node with native message queries enabled.
// Prints one JSON line: {"url":"http://127.0.0.1:<port>/","port":<port>}
const path = require('node:path')
const { startFixture } = require(path.resolve(__dirname, '../../../tests/fixtures/ui-fixture.cjs'))

;(async () => {
  const fixture = await startFixture(0, { nativeQueries: true, includeHistoryAssistant: true })
  console.log(JSON.stringify({ url: fixture.url, port: Number(new URL(fixture.url).port) }))
})()
