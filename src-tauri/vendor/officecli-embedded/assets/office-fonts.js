function hasWindowsWordFace(context, family) {
  if (!navigator.userAgent.includes('Windows')) return false;
  context.save();
  const probe = 'Wim 0123';
  context.font = '100 48px monospace';
  const fallback = context.measureText(probe).width;
  context.font = '100 48px "' + family + '", monospace';
  const available = context.measureText(probe).width !== fallback;
  context.restore();
  return available;
}

function applyWindowsNotoFace(body) {
  if (!navigator.userAgent.includes('Windows')) return;
  const candidates = [];
  for (const run of body.querySelectorAll('.hcd-run,.hcd-ppt-run')) {
    const style = getComputedStyle(run);
    const family = style.fontFamily.split(',')[0].trim().replace(/["']/g, '').toLowerCase();
    const weight = Number(style.fontWeight);
    if (family === 'noto sans sc' && (weight === 400 || weight === 700)) {
      candidates.push({run, bold: weight === 700});
    }
  }
  if (!candidates.length) return;
  // 使用本地已安装字体。打印前的 load 回调可以同步完成全部排版。
  const context = body.ownerDocument.createElement('canvas').getContext('2d');
  if (!context) return;
  if (!hasWindowsWordFace(context, 'Noto Sans SC Thin')) return;
  for (const {run, bold} of candidates) {
    run.style.fontFamily = '"Noto Sans SC Thin",' + getComputedStyle(run).fontFamily;
    run.style.fontWeight = '100';
    run.style.fontVariationSettings = '"wght" 100';
    if (bold) run.style.webkitTextStrokeWidth = '0.0286em';
    const paragraph = run.closest('.hcd-slide-text');
    if (paragraph && !paragraph.style.lineHeight) paragraph.style.lineHeight = '1.2';
  }
}
