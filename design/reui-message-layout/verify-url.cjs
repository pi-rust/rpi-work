// Preserve the runtime-ready fixture when a regression selects another view.
function withQuery(url, values) {
  const target = new URL(url);
  for (const [name, value] of Object.entries(values)) {
    if (value == null) target.searchParams.delete(name);
    else target.searchParams.set(name, value);
  }
  return target.href;
}
module.exports = { withQuery };
