export function adaptPanelLifecycle(source, replaceOne) {
  source = replaceOne(
    source,
    "return ge?ge.downloads.push(s):s.catch(en),!0",
    "return ge?ge.downloads.push(ge.request.iywUserAction?s.catch(error=>{en(error);throw error}):s):s.catch(en),!0"
  )
  source = replaceOne(
    source,
    "bC();const iywStorageSource=",
    'bC();window.addEventListener("keydown",event=>{if(event.key==="Escape"&&!document.querySelector("dialog[open]")&&tt.getHostContext()?.displayMode==="fullscreen"){event.preventDefault();event.stopImmediatePropagation();tt.requestDisplayMode({mode:"inline"}).catch(en)}},true);const iywStorageSource='
  )
  source = replaceOne(
    source,
    "async function b_(e,t){if(ge){",
    "async function b_(e,t){if(ge&&!ge.request.iywUserAction){"
  )
  source = replaceOne(
    source,
    "globalThis.__craftWrite=(e,t)=>{if(!ge){",
    "globalThis.__craftWrite=(e,t)=>{if(!ge||ge.request.iywUserAction){"
  )
  source = replaceOne(
    source,
    "async function yC(){",
    "let iywStorage;async function yC(){"
  )
  source = replaceOne(
    source,
    "y_=!0,Eo.loaded=!0",
    "await iywStorage.ready(l),y_=!0,Eo.loaded=!0"
  )
  source = replaceOne(
    source,
    "tt.onteardown=async()=>{Po=!0,",
    "tt.onteardown=async()=>{await iywStorage?.flush();Po=!0,"
  )
  source = replaceOne(
    source,
    'await Ld("_editor_attach",{})',
    "await iywStorage.attach(message=>To(message),()=>Po)"
  )
  return replaceOne(
    source,
    "Eo.automationReady=!0;!Po;",
    "Eo.automationReady=!0,iywStorage.startCompact();!Po;"
  )
}
