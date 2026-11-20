// Dock the project inspector like a split pane. The width lives on the shell as
// a CSS variable so the conversation margin and panel stay in sync.
(() => {
  const handle = document.querySelector('.project-files-modal .project-files-resize');
  const shell = document.querySelector('.app-shell');
  if (!handle || !shell || handle.dataset.bound === '1') return;
  handle.dataset.bound = '1';
  const MIN = 320, MAX = 480;
  const clamp = width => Math.max(MIN, Math.min(MAX, Math.round(width)));
  const current = () => parseFloat(getComputedStyle(shell).getPropertyValue('--project-files-width')) || 392;
  const set = width => shell.style.setProperty('--project-files-width', `${clamp(width)}px`);
  let dragging = false, startX = 0, startWidth = 0;
  handle.addEventListener('pointerdown', event => {
    if (!shell.classList.contains('project-files-docked')) return;
    dragging = true; startX = event.clientX; startWidth = current();
    handle.setPointerCapture(event.pointerId); event.preventDefault();
  });
  handle.addEventListener('pointermove', event => {
    if (dragging) set(startWidth - (event.clientX - startX));
  });
  const end = () => { dragging = false; };
  handle.addEventListener('pointerup', end);
  handle.addEventListener('pointercancel', end);
  handle.addEventListener('keydown', event => {
    if (!shell.classList.contains('project-files-docked')) return;
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    set(current() + (event.key === 'ArrowLeft' ? 16 : -16));
  });
  return true;
})();
