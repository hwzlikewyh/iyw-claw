// Word 的“与下段同页”至少保留下一段的首行，避免标题孤立在页底。
function wordKeepNextFits(blocks, index, content) {
  if (blocks[index].node.dataset.hcdKeepNext !== 'true') return true;
  const probes = [];
  const bottom = content.getBoundingClientRect().bottom;
  const maxChain = 50;
  try {
    for (let offset = 1; offset <= maxChain; offset++) {
      const next = blocks[index + offset];
      if (!next || next.before || next.section !== blocks[index].section) return true;
      const probe = next.node.cloneNode(true);
      content.append(probe); probes.push(probe);
      if (probe.dataset.hcdKeepNext === 'true') continue;
      const row = probe.querySelector('tr');
      if (row) return row.getBoundingClientRect().bottom <= bottom + 1;
      const walker = probe.ownerDocument.createTreeWalker(probe, NodeFilter.SHOW_TEXT);
      let text;
      while ((text = walker.nextNode())) {
        if (!text.textContent.trim()) continue;
        const range = probe.ownerDocument.createRange();
        range.selectNodeContents(text);
        const first = [...range.getClientRects()].find(rect => rect.height > 0);
        if (first) return first.bottom <= bottom + 1;
      }
      return probe.getBoundingClientRect().bottom <= bottom + 1;
    }
    return true;
  } finally {
    probes.forEach(probe => probe.remove());
  }
}
