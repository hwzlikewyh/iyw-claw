const COLORS = [
  "foreground",
  "muted-foreground",
  "background",
  "primary",
  "primary-foreground",
  "card",
  "border",
  "chart-1",
  "chart-2",
  "chart-3",
  "chart-4",
  "chart-5",
]
export const WIDGET_CSP =
  "default-src 'none'; base-uri 'none'; form-action 'none'; object-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; connect-src 'none'; img-src data: blob:; media-src data: blob:; font-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'"
export function widgetTheme() {
  const css = getComputedStyle(document.documentElement)
  return {
    scheme: css.colorScheme,
    variables: Object.fromEntries(
      COLORS.map((color) => [
        `--${color}`,
        css.getPropertyValue(`--${color}`).trim(),
      ])
    ),
  }
}
export function widgetDocument(
  source: string,
  token: string,
  initialState: Record<string, unknown> = {}
): string {
  const initial = JSON.stringify(initialState).replace(/</g, "\\u003c")
  const runtime = `(() => {
    const token=${JSON.stringify(token)}; let state=${initial},loaded=false; const listeners=new Set(), themes=new Set();
    const post=(type,value)=>parent.postMessage({channel:'iyw-widget',token,type,...value},'*');
    const request=()=>post('resize',{height:document.documentElement.scrollHeight});
    addEventListener('message',event=>{if(event.source!==parent||event.data?.token!==token)return;
      if(event.data.type==='ping'&&loaded){post('ready',{});request();return;}
      const d=event.data;if(d.type==='state'&&JSON.stringify(state)!==JSON.stringify(d.state||{})){state=d.state||{};for(const fn of listeners)fn(state);}
      if(d.type==='theme'){document.documentElement.style.colorScheme=d.theme.scheme;for(const [key,value]of Object.entries(d.theme.variables))document.documentElement.style.setProperty(key,value);for(const fn of themes)fn(d.theme);}
    });
    window.iyw=Object.freeze({getState:()=>state,onState:fn=>{listeners.add(fn);fn(state);return()=>listeners.delete(fn)},setState:patch=>{if(!patch||typeof patch!=='object'||Array.isArray(patch))return;state={...state,...patch};post('state',{state})},suggest:text=>post('suggest',{text:String(text).slice(0,2000)}),ask:text=>post('suggest',{text:String(text).slice(0,2000)}),onTheme:fn=>{themes.add(fn);return()=>themes.delete(fn)}});
    addEventListener('DOMContentLoaded',()=>{loaded=true;new ResizeObserver(request).observe(document.documentElement);post('ready',{});request();});
    addEventListener('error',()=>post('error',{}));addEventListener('unhandledrejection',()=>post('error',{}));
  })();`
  const inner = `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="${WIDGET_CSP}"><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body{margin:0;background:transparent;color:var(--foreground);font:13px/1.6 sans-serif}body{padding:16px}*{box-sizing:border-box}button,input,select{font:inherit;color:inherit;background:var(--card);border:1px solid var(--border);border-radius:6px;padding:4px 8px}svg,canvas,img{max-width:100%}:focus-visible{outline:2px solid var(--primary)}</style><script>${runtime}</script></head><body>${source}</body></html>`
  return widgetShell(inner, token)
}

// 可信外层只转发消息；frame-src 限制内层自身跳转，模型脚本无法修改外层 CSP。
function widgetShell(inner: string, token: string) {
  const policy = WIDGET_CSP.replace("frame-src 'none'", "frame-src about:")
  const encoded = JSON.stringify(inner).replace(/</g, "\\u003c")
  const relay = `(() => {
    const token=${JSON.stringify(token)}, frame=document.createElement('iframe');
    frame.setAttribute('sandbox','allow-scripts');frame.setAttribute('referrerpolicy','no-referrer');
    frame.setAttribute('allow',"camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'");
    frame.style.cssText='display:block;width:100%;height:96px;border:0';
    addEventListener('message',event=>{
      const data=event.data;if(!data||data.token!==token)return;
      if(event.source===frame.contentWindow&&data.channel==='iyw-widget'){
        if(data.type==='resize'&&Number.isFinite(data.height))frame.style.height=Math.max(96,Math.min(640,data.height))+'px';
        parent.postMessage(data,'*');
      }else if(event.source===parent){frame.contentWindow?.postMessage(data,'*');}
    });
    frame.srcdoc=${encoded};document.body.appendChild(frame);
  })();`
  return `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="${policy}"><style>html,body{margin:0;background:transparent}</style></head><body><script>${relay}</script></body></html>`
}
