// 在当前沙箱内隔离文档样式，避免读取另一个不透明来源的 iframe。
LazyHcdViewer.prototype.createDocument = function (styles) {
  const root = frame.shadowRoot || frame.attachShadow({ mode: "open" })
  const html = document.createElement("html")
  const head = document.createElement("head")
  const body = document.createElement("body")
  const style = document.createElement("style")
  style.textContent = `
    html{display:block;background:#eef1f5;min-height:100%}
    body{box-sizing:border-box;max-width:max-content;min-width:min(100%,960px);margin:24px auto;padding:24px;background:#fff;color:#111;box-shadow:0 3px 18px #0002}
    .hcd-lazy-placeholder{display:block;box-sizing:border-box;min-height:480px;margin-bottom:12px;background:#f4f6f8;border:1px solid #d7dce2}
    .hcd-lazy-error{padding:18px;color:#9f1239;background:#fff1f2;border:1px solid #fda4af}
    #hcd-lazy-tail{height:2px}
  ` + styles.replace(/asset:\/\/sha256\/([0-9a-f]{64})/g, (source, hash) => this.assets.get(hash) ?? source)
  body.dataset.hcdProfile = this.manifest.profile
  body.dataset.hcdSourceFormat = this.manifest.source.format
  body.dataset.hcdRevision = String(this.revision)
  body.dataset.hcdTextHitboxes = "off"
  body.dataset.hcdImageHitboxes = "off"
  head.append(style)
  html.append(head, body)
  root.replaceChildren(html)
  this.doc = { body, createElement: (tag) => document.createElement(tag) }
  this.tail = document.createElement("div")
  this.tail.id = "hcd-lazy-tail"
  body.append(this.tail)
  this.observer = new IntersectionObserver(
    (entries) => this.onIntersection(entries),
    { root: frame, rootMargin: "120% 0px 120% 0px" }
  )
}

// 上游示例的分片统计不属于产品提示；异步渲染错误交给宿主统一显示。
LazyHcdViewer.prototype.updateStatus = function () {}
LazyHcdViewer.prototype.fail = function (error) {
  if (!this.abort.signal.aborted) window.officeReportError(error)
}
const showChunkError = LazyHcdViewer.prototype.showChunkError
LazyHcdViewer.prototype.showChunkError = function (slot, error) {
  if (this.abort.signal.aborted) return
  showChunkError.call(this, slot, error)
  this.fail(error)
}

let officeViewer
const RESIDENT_CHUNKS = 12
window.officeLoad = async function (base) {
  const top = frame.scrollTop
  officeViewer?.close()
  officeViewer = new LazyHcdViewer(new URL(base), undefined, RESIDENT_CHUNKS)
  await officeViewer.open()
  frame.scrollTo(0, top)
}
window.officeDispose = () => officeViewer?.close()
