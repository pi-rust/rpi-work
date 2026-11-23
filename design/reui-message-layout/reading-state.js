// Reading state belongs to this page, not the persisted chat/runtime protocol.
const readingState = (() => {
  const positions = new Map();
  const anchors = () => [...transcript.querySelectorAll('.prompt, .assistant > p, .reply h2, .reply > p, .reply > ul, .activity > summary, .message-code, .turn-heading, .turn-response')];

  function capture() {
    if (!session || !transcript.childElementCount || historyPreview.blocked() || $('.workspace').dataset.page === 'resources' || $('.workspace').dataset.page === 'automations') return;
    const viewport = conversation.getBoundingClientRect();
    const nodes = anchors();
    const index = nodes.findIndex(node => {
      const rect = node.getBoundingClientRect();
      return rect.bottom > viewport.top + 8 && rect.top < viewport.bottom;
    });
    positions.set(session.id, {
      top: conversation.scrollTop,
      bottom: conversation.scrollHeight - conversation.clientHeight - conversation.scrollTop < 48,
      open: [...transcript.querySelectorAll('details[data-activity][open]')].map(node => node.dataset.activity),
      anchor: index < 0 ? null : {
        index, text: nodes[index].textContent.slice(0, 80),
        offset: nodes[index].getBoundingClientRect().top - viewport.top,
      },
    });
  }

  function restore() {
    if (historyPreview.blocked()) return;
    const saved = positions.get(session?.id);
    if (!saved) { scrollToLatestTurn(); return; }
    for (const detail of transcript.querySelectorAll('details[data-activity]')) detail.open = saved.open.includes(detail.dataset.activity);
    if (saved.bottom) { conversation.scrollTop = conversation.scrollHeight; return; }
    conversation.scrollTop = saved.top;
    const node = saved.anchor && anchors()[saved.anchor.index];
    if (node && node.textContent.slice(0, 80) === saved.anchor.text) {
      conversation.scrollTop += node.getBoundingClientRect().top - conversation.getBoundingClientRect().top - saved.anchor.offset;
    }
  }

  return { capture, restore };
})();
