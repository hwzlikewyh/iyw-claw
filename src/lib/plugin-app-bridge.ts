import type { PluginAppResourceMeta } from "@/lib/api"

export type JsonRpcId = string | number

export type PluginAppMessage = {
  jsonrpc: "2.0"
  id?: JsonRpcId
  method: string
  params?: unknown
}

export type PluginAppLaunch = {
  instanceId: string
  conversationId: number
  leaseToken: string
  nonce: string
  launchPayload?: unknown
}

type PluginAppResponse = {
  accepted: boolean
  result?: unknown
  error?: { code: number; message: string }
}

const MAX_MESSAGE_BYTES = 256 * 1024
const METHODS = new Set([
  "initialize",
  "ui/initialize",
  "ui/open-link",
  "ui/message",
  "ui/request-display-mode",
  "ui/update-model-context",
  "ui/resource-teardown",
  "tools/call",
  "resources/list",
  "resources/read",
  "notifications/message",
  "ping",
  "ui/notifications/initialized",
  "ui/notifications/size-changed",
  "ui/notifications/sandbox-proxy-ready",
  "ui/notifications/request-teardown",
])
const HOST_METHODS = new Set([
  "ui/notifications/tool-input",
  "ui/notifications/tool-result",
  "ui/notifications/host-context-changed",
  "ui/resource-teardown",
  "ui/notifications/sandbox-resource-ready",
])

export class PluginAppBridge {
  private readonly onMessage: (
    message: PluginAppMessage
  ) => Promise<PluginAppResponse>
  private readonly onInitialized?: () => void
  private port: MessagePort | null = null
  private initialized = false

  constructor(
    onMessage: (message: PluginAppMessage) => Promise<PluginAppResponse>,
    onInitialized?: () => void
  ) {
    this.onMessage = onMessage
    this.onInitialized = onInitialized
  }

  attach(port: MessagePort): void {
    this.port?.close()
    this.port = port
    port.onmessage = (message) => void this.handleMessage(message.data)
    port.start()
  }

  send(message: Record<string, unknown>): boolean {
    if (!this.port || !withinLimit(message)) return false
    this.port.postMessage(message)
    return true
  }

  sendResourceReady(
    html: string,
    resourceMeta?: PluginAppResourceMeta
  ): boolean {
    if (!this.port || byteLength(html) > 8 * 1024 * 1024) return false
    const message = {
      jsonrpc: "2.0",
      method: "ui/notifications/sandbox-resource-ready",
      params: { html, ...resourceMeta },
    }
    if (!withinLimit(message)) return false
    this.port.postMessage(message)
    return true
  }

  notify(message: Record<string, unknown>): boolean {
    return this.initialized && this.send(message)
  }

  dispose(): void {
    if (this.initialized) {
      this.send({
        jsonrpc: "2.0",
        id: `teardown-${Date.now()}`,
        method: "ui/resource-teardown",
        params: { reason: "host_unmount" },
      })
    }
    this.initialized = false
    const port = this.port
    this.port = null
    if (port) {
      globalThis.setTimeout(() => port.close(), 250)
    }
  }

  private async handleMessage(value: unknown): Promise<void> {
    const request = parseMessage(value)
    if (!request) return
    try {
      const response = await this.onMessage(request)
      if (
        !response.error &&
        request.method === "ui/notifications/initialized"
      ) {
        this.initialized = true
        this.onInitialized?.()
      }
      if (request.id === undefined) return
      this.send(
        response.error
          ? { jsonrpc: "2.0", id: request.id, error: response.error }
          : { jsonrpc: "2.0", id: request.id, result: response.result ?? {} }
      )
    } catch (error) {
      if (request.id === undefined) return
      this.send({
        jsonrpc: "2.0",
        id: request.id,
        error: {
          code: -32000,
          message:
            error instanceof Error
              ? error.message
              : "Plugin app request failed",
        },
      })
    }
  }
}

function parseMessage(value: unknown): PluginAppMessage | null {
  if (!value || typeof value !== "object" || !withinLimit(value)) return null
  const item = value as Record<string, unknown>
  if (item.jsonrpc !== "2.0" || typeof item.method !== "string") return null
  if (!METHODS.has(item.method)) return null
  if (
    item.id !== undefined &&
    typeof item.id !== "string" &&
    typeof item.id !== "number"
  ) {
    return null
  }
  return {
    jsonrpc: "2.0",
    id: item.id as JsonRpcId | undefined,
    method: item.method,
    params: item.params,
  }
}

function withinLimit(value: unknown): boolean {
  try {
    return (
      new TextEncoder().encode(JSON.stringify(value)).byteLength <=
      MAX_MESSAGE_BYTES
    )
  } catch {
    return false
  }
}

function byteLength(value: string): number {
  return new TextEncoder().encode(value).byteLength
}

export function buildPluginAppProxyDocument(): string {
  const methods = JSON.stringify([...METHODS, ...HOST_METHODS])
  return `<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; frame-src data: blob:; object-src 'none'; base-uri 'self';"><script>
const MAX=${MAX_MESSAGE_BYTES};
const RESOURCE_MAX=8*1024*1024;
const METHODS=new Set(${methods});
let port=null;
let frame=null;
function valid(value){
  try{
    if(!value||typeof value!=="object"||value.jsonrpc!=="2.0"||new TextEncoder().encode(JSON.stringify(value)).byteLength>MAX)return false;
    if(typeof value.method==="string")return METHODS.has(value.method);
    return (typeof value.id==="string"||typeof value.id==="number")&&("result" in value||"error" in value)
  }catch{return false}
}
function forward(value){if(port&&frame&&valid(value))frame.contentWindow?.postMessage(value,"*")}
function loadFrame(html,meta){
  if(frame||typeof html!=="string"||new TextEncoder().encode(html).byteLength>RESOURCE_MAX)return;
  frame=document.createElement("iframe");
  frame.setAttribute("sandbox","allow-scripts");
  const permissions=meta?.permissions&&typeof meta.permissions==="object"?meta.permissions:{};
  const allow=["camera","microphone","geolocation","clipboard-write"].filter(name=>Boolean(permissions[name==='clipboard-write'?'clipboardWrite':name])).join(";");
  if(allow)frame.setAttribute("allow",allow);
  frame.style.cssText="border:0;width:100%;height:100%;display:block";
  const csp=meta?.csp&&typeof meta.csp==="object"?meta.csp:{};
  const list=(key)=>Array.isArray(csp[key])?csp[key].filter(value=>typeof value==="string"&&/^https:\/\//i.test(value)&&!/[\s<>"']/.test(value)):[];
  const connect=list("connectDomains");
  const resources=list("resourceDomains");
  const frames=list("frameDomains");
  const base=list("baseUriDomains");
  const sources=(fallback,values)=>values.length?values.join(" "):fallback;
  const policy=["default-src 'none'","script-src 'unsafe-inline' "+resources.join(" "),"style-src 'unsafe-inline' "+resources.join(" "),"img-src data: blob: "+resources.join(" "),"font-src data: "+resources.join(" "),"media-src data: blob: "+resources.join(" "),"connect-src "+sources("'none'",connect),"frame-src "+sources("'none'",frames),"object-src 'none'","base-uri 'self' "+base.join(" ")].join(";");
  frame.srcdoc='<meta http-equiv="Content-Security-Policy" content="'+policy+'">'+html;
  document.body.style.margin="0";
  document.body.appendChild(frame);
  window.addEventListener("message",event=>{if(event.source===frame.contentWindow&&port&&valid(event.data))port.postMessage(event.data)});
}
window.addEventListener("message",event=>{
  if(event.source!==window.parent||event.data?.type!=="iyw-plugin-app-init"||frame)return;
  port=event.ports[0]||null;
  if(!port)return;
  port.onmessage=event=>{
    const value=event.data;
    if(value?.method==="ui/notifications/sandbox-resource-ready")loadFrame(value.params?.html,value.params);
    else forward(value);
  };
  port.start();
  port.postMessage({jsonrpc:"2.0",method:"ui/notifications/sandbox-proxy-ready",params:{}});
});
</script>`
}
