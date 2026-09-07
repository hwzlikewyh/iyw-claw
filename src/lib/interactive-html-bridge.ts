import {
  type HtmlErrorKey,
  HTML_MESSAGE_ID_CHARS,
  HTML_MAX_HEIGHT,
  HTML_MIN_HEIGHT,
  HTML_SUBMIT_TIMEOUT_MS,
  MAX_HTML_RESULT_BYTES,
  serializedBytes,
} from "./interactive-html"

type BridgeOptions = {
  frame: HTMLIFrameElement
  nonce: string
  waiting: boolean
  onReady: () => void
  onError: (message: HtmlErrorKey) => void
  onSubmit: (data: unknown) => Promise<void>
}

const CSP =
  "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"

const SUBMIT_SCRIPT = `  Object.defineProperty(window, 'iyw', { value: Object.freeze({
    submit(data) {
      if (!port) return Promise.reject(new Error('The page is not ready'));
      if (completed) return Promise.reject(new Error('The interaction has already finished'));
      if (pending) return Promise.reject(new Error('A response is already being submitted'));
      let value;
      try {
        const text = JSON.stringify(data);
        if (text === undefined || new TextEncoder().encode(text).length > limit) throw new Error('Response must be JSON up to 64 KiB');
        value = JSON.parse(text);
      } catch(error) { return Promise.reject(error); }
      return new Promise((resolve, reject) => {
        const id = String(++counter);
        pending = { id, resolve, reject };
        timer = setTimeout(() => { pending = null; reject(new Error('Submission timed out; you can retry')); }, ${HTML_SUBMIT_TIMEOUT_MS});
        send({ type: 'submit', id, data: value });
      });
    }
  }), writable: false, configurable: false });
`

// 引导代码在用户文档之前运行，不将 HTML 拼入脚本或带入宿主 origin。
export function interactiveHtmlDocument(html: string, nonce: string): string {
  return `<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${CSP}"><script>
(() => {
  const nonce = ${JSON.stringify(nonce)};
  const limit = ${MAX_HTML_RESULT_BYTES};
  let port, pending, timer, counter = 0, completed = false, pageError = false;
  const send = value => port?.postMessage(value);
  const reject = message => { clearTimeout(timer); pending?.reject(new Error(message)); pending = null; };
${SUBMIT_SCRIPT}
  const ready = () => {
    send({ type: 'ready' });
    if (pageError) send({ type: 'page-error' });
    let scheduled = false;
    new ResizeObserver(() => {
      if (scheduled) return;
      scheduled = true;
      requestAnimationFrame(() => { scheduled = false; send({ type: 'resize', height: document.documentElement.scrollHeight }); });
    }).observe(document.documentElement);
  };
  window.addEventListener('message', event => {
    if (event.source !== parent || event.data?.type !== 'iyw-html-connect' || event.data.nonce !== nonce || port || !event.ports[0]) return;
    port = event.ports[0];
    port.onmessage = ({ data }) => {
      if (data?.type !== 'result' || !pending || pending.id !== data.id) return;
      if (data.error) { reject(data.error); return; }
      clearTimeout(timer); completed = true; pending.resolve(); pending = null;
    };
    port.start();
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', ready, { once: true });
    else ready();
  });
  const reportError = () => { pageError = true; send({ type: 'page-error' }); };
  window.addEventListener('error', reportError);
  window.addEventListener('unhandledrejection', reportError);
  parent.postMessage({ type: 'iyw-html-ready', nonce }, '*');
})();
</script>${html}`
}

export function connectInteractiveHtml(options: BridgeOptions): () => void {
  let port: MessagePort | null = null
  let disposed = false
  const handle = createReceiver(options, (message) => {
    if (!disposed) port?.postMessage(message)
  })
  const onMessage = (event: MessageEvent) => {
    if (
      disposed ||
      port ||
      event.source !== options.frame.contentWindow ||
      event.data?.type !== "iyw-html-ready" ||
      event.data.nonce !== options.nonce
    )
      return
    const channel = new MessageChannel()
    port = channel.port1
    port.onmessage = (message) => {
      if (!disposed) void handle(message.data)
    }
    port.start()
    options.frame.contentWindow?.postMessage(
      { type: "iyw-html-connect", nonce: options.nonce },
      "*",
      [channel.port2]
    )
  }
  window.addEventListener("message", onMessage)
  return () => {
    disposed = true
    window.removeEventListener("message", onMessage)
    port?.close()
  }
}

function resize(frame: HTMLIFrameElement, height: unknown) {
  if (typeof height !== "number" || !Number.isFinite(height)) return
  frame.style.height = `${Math.round(Math.min(HTML_MAX_HEIGHT, Math.max(HTML_MIN_HEIGHT, height)))}px`
}

function createReceiver(
  options: BridgeOptions,
  send: (value: unknown) => void
) {
  let submitting = false
  let completed = false
  const handle = async (message: Record<string, unknown>) => {
    if (!message || typeof message !== "object") return
    if (message.type === "ready") options.onReady()
    if (message.type === "page-error") options.onError("pageError")
    if (message.type === "resize") resize(options.frame, message.height)
    if (
      message.type !== "submit" ||
      typeof message.id !== "string" ||
      message.id.length > HTML_MESSAGE_ID_CHARS
    )
      return
    if (completed) {
      send({ type: "result", id: message.id })
      return
    }
    if (submitting) return
    submitting = true
    try {
      if (!options.waiting)
        throw new Error("This page does not collect responses")
      if (serializedBytes(message.data) > MAX_HTML_RESULT_BYTES)
        throw new Error("Response exceeds 64 KiB")
      await options.onSubmit(message.data)
      completed = true
      send({ type: "result", id: message.id })
    } catch (error) {
      options.onError("submitError")
      send({
        type: "result",
        id: message.id,
        error: String(error),
      })
    } finally {
      submitting = false
    }
  }

  return handle
}
