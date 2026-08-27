export type PluginAppMessage = {
  method: "ui/initialize" | "ui/message" | "ui/resize" | "ui/teardown"
  params?: unknown
}

export type PluginAppLaunch = {
  instanceId: string
  leaseToken: string
  nonce: string
}

const MAX_MESSAGE_BYTES = 256 * 1024

export class PluginAppBridge {
  private readonly launch: PluginAppLaunch
  private readonly onMessage: (message: PluginAppMessage) => void
  private port: MessagePort | null = null

  constructor(
    launch: PluginAppLaunch,
    onMessage: (message: PluginAppMessage) => void
  ) {
    this.launch = launch
    this.onMessage = onMessage
  }

  attach(port: MessagePort): void {
    this.port?.close()
    this.port = port
    port.onmessage = (message) => this.handleMessage(message.data)
    port.start()
    this.send({
      method: "ui/initialize",
      params: { instanceId: this.launch.instanceId },
    })
  }

  send(message: PluginAppMessage): boolean {
    if (!this.port) return false
    const envelope = {
      instanceId: this.launch.instanceId,
      leaseToken: this.launch.leaseToken,
      nonce: this.launch.nonce,
      ...message,
    }
    const encoded = JSON.stringify(envelope)
    if (new TextEncoder().encode(encoded).byteLength > MAX_MESSAGE_BYTES) {
      return false
    }
    this.port.postMessage(envelope)
    return true
  }

  dispose(): void {
    this.send({ method: "ui/teardown" })
    this.port?.close()
    this.port = null
  }

  private handleMessage(value: unknown): void {
    if (!value || typeof value !== "object") return
    const envelope = value as Record<string, unknown>
    if (
      envelope.instanceId !== this.launch.instanceId ||
      envelope.leaseToken !== this.launch.leaseToken ||
      envelope.nonce !== this.launch.nonce ||
      typeof envelope.method !== "string"
    ) {
      return
    }
    if (
      envelope.method !== "ui/initialize" &&
      envelope.method !== "ui/message" &&
      envelope.method !== "ui/resize" &&
      envelope.method !== "ui/teardown"
    ) {
      return
    }
    this.onMessage({
      method: envelope.method,
      params: envelope.params,
    })
  }
}

export function buildPluginAppProxyDocument(): string {
  return `<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; child-src data: blob:;"><script>
const MAX=262144;
let port=null;
let launch=null;
let frame=null;
function bytes(value){return new TextEncoder().encode(JSON.stringify(value)).byteLength}
function forward(value){
  if(!port||!value||typeof value!=="object")return;
  if(value.instanceId!==launch.instanceId||value.leaseToken!==launch.leaseToken||value.nonce!==launch.nonce)return;
  if(!["ui/initialize","ui/message","ui/resize","ui/teardown"].includes(value.method)||bytes(value)>MAX)return;
  if(frame&&frame.contentWindow)frame.contentWindow.postMessage(value,"*");
}
window.addEventListener("message",event=>{
  if(event.source!==window.parent||event.data?.type!=="iyw-plugin-app-init")return;
  const incoming=event.data;
  if(typeof incoming.html!=="string"||bytes(incoming.html)>MAX*4)return;
  launch={instanceId:incoming.instanceId,leaseToken:incoming.leaseToken,nonce:incoming.nonce};
  port=event.ports[0]||null;
  if(!port)return;
  frame=document.createElement("iframe");
  frame.setAttribute("sandbox","allow-scripts");
  frame.style.cssText="border:0;width:100%;height:100%;display:block";
  frame.srcdoc='<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'unsafe-inline\'; style-src \'unsafe-inline\'; img-src data: blob:; font-src data:; connect-src \'none\';">'+incoming.html;
  document.body.style.margin="0";
  document.body.appendChild(frame);
  window.addEventListener("message",event=>{
    if(event.source===frame.contentWindow&&port&&event.data)port.postMessage(event.data);
  });
  port.onmessage=event=>forward(event.data);
  port.start();
});
</script>`
}
