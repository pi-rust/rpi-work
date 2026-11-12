// Keep the selected category visible without moving keyboard focus or vertical scroll.
requestAnimationFrame(() => {
  for (const rail of document.querySelectorAll('.settings-page .settings-rail')) {
    const selected = rail.querySelector('[aria-current="page"]');
    if (!selected) continue;
    const bounds = rail.getBoundingClientRect();
    const item = selected.getBoundingClientRect();
    const inset = 16;
    if (item.left < bounds.left + inset) rail.scrollLeft += item.left - bounds.left - inset;
    else if (item.right > bounds.right - inset) rail.scrollLeft += item.right - bounds.right + inset;
  }
});
