(() => {
  if (window.top !== window) return;
  const limit = 2000;
  const send = value => window.ipc.postMessage(JSON.stringify(value));
  const describe = value => String(value instanceof Error ? value.stack || value.message : value).slice(0, limit);
  window.addEventListener('error', event => send({ type: 'error', message: describe(event.message), source: String(event.filename).slice(0, limit) }));
  window.addEventListener('unhandledrejection', event => send({ type: 'error', message: describe(event.reason) }));
  const original = console.error;
  console.error = (...args) => { send({ type: 'error', message: args.map(describe).join(' ').slice(0, limit) }); original.apply(console, args); };
  window.addEventListener('keydown', event => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
      event.preventDefault(); send({ type: 'shortcut', key: 'find' });
    }
  }, true);
  let cancel = () => {};
  let searchTerm = '', matchIndex = -1;
  Object.defineProperty(window, '__iywPageFind', { value: (query, backwards) => {
    if (!query) { getSelection()?.removeAllRanges(); searchTerm = ''; matchIndex = -1; return; }
    if (typeof window.find === 'function') { window.find(query, false, backwards, true); return; }
    if (!document.body) return;
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const matches = [];
    const text = query.toLowerCase();
    while (walker.nextNode()) {
      const node = walker.currentNode, element = node.parentElement;
      if (!element || /^(SCRIPT|STYLE|NOSCRIPT)$/.test(element.tagName) || !element.getClientRects().length) continue;
      let offset = -1;
      const value = node.textContent.toLowerCase();
      while ((offset = value.indexOf(text, offset + 1)) >= 0) matches.push({ node, offset });
    }
    if (!matches.length) return;
    if (searchTerm !== text) matchIndex = backwards ? matches.length : -1;
    searchTerm = text;
    matchIndex = (matchIndex + (backwards ? -1 : 1) + matches.length) % matches.length;
    const match = matches[matchIndex], range = document.createRange();
    range.setStart(match.node, match.offset); range.setEnd(match.node, match.offset + query.length);
    const selection = getSelection(); selection.removeAllRanges(); selection.addRange(range);
    match.node.parentElement.scrollIntoView({ block: 'center', inline: 'nearest' });
  } });
  Object.defineProperty(window, '__iywPageCancel', { value: () => cancel() });
  Object.defineProperty(window, '__iywPagePick', { value: token => {
    cancel();
    const overlay = document.createElement('div');
    overlay.style.cssText = 'position:fixed;pointer-events:none;z-index:2147483647;border:2px solid #7c3aed;background:#7c3aed22;box-sizing:border-box';
    document.documentElement.append(overlay);
    const move = event => {
      const target = document.elementFromPoint(event.clientX, event.clientY);
      if (!target) return;
      const rect = target.getBoundingClientRect();
      Object.assign(overlay.style, { left: rect.left + 'px', top: rect.top + 'px', width: rect.width + 'px', height: rect.height + 'px' });
    };
    const click = event => {
      event.preventDefault(); event.stopImmediatePropagation();
      const target = document.elementFromPoint(event.clientX, event.clientY);
      if (!target) return;
      const rect = target.getBoundingClientRect();
      const selector = target.id ? '#' + CSS.escape(target.id) : target.tagName.toLowerCase() + Array.from(target.classList).slice(0, 3).map(name => '.' + CSS.escape(name)).join('');
      cancel();
      send({ type: 'picked', token, url: location.href, title: document.title,
        tag: target.tagName.toLowerCase(), selector, role: target.getAttribute('role'),
        text: (target.innerText || target.textContent || '').slice(0, limit),
        region: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } });
    };
    const key = event => {
      if (event.key !== 'Escape') return;
      event.preventDefault(); cancel(); send({ type: 'picked', token, cancelled: true });
    };
    cancel = () => {
      overlay.remove(); document.removeEventListener('pointermove', move, true);
      document.removeEventListener('click', click, true); document.removeEventListener('keydown', key, true);
      cancel = () => {};
    };
    document.addEventListener('pointermove', move, true);
    document.addEventListener('click', click, true);
    document.addEventListener('keydown', key, true);
  } });
})();
