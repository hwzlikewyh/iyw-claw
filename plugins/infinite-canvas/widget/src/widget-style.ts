export function installWidgetStyle(): void {
  const style = document.createElement("style")
  style.textContent = `
  :root { color-scheme: dark; font-family: system-ui, sans-serif; }
  * { box-sizing: border-box; }
  body { margin: 0; min-width: 320px; min-height: 240px; background: #101318; color: #f4f7fb; }
  .shell { display: grid; grid-template-rows: auto 1fr auto; min-height: 100vh; }
  .toolbar { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; padding: 10px 12px; border-bottom: 1px solid #303845; background: #181d26; }
  .toolbar button { border: 1px solid #485363; border-radius: 5px; padding: 6px 10px; background: #232a35; color: inherit; cursor: pointer; }
  .toolbar button:hover { background: #2d3745; }
  .title { margin-right: auto; font-weight: 600; }
  .surface { position: relative; overflow: auto; min-height: 320px; background-color: #101318; background-image: radial-gradient(#313a48 1px, transparent 1px); background-size: 20px 20px; }
  .scene { position: relative; width: 1600px; height: 1000px; transform-origin: 0 0; }
  .node { position: absolute; overflow: hidden; border: 1px solid #5c6b80; border-radius: 5px; padding: 10px; background: #222a36; color: #eef3f9; white-space: pre-wrap; cursor: grab; touch-action: none; }
  .node:active { cursor: grabbing; }
  .node.selected { border-color: #72b7ff; box-shadow: 0 0 0 2px #72b7ff44; }
  .status { padding: 7px 12px; border-top: 1px solid #303845; color: #aeb9c8; font-size: 12px; }
  .error { color: #ff9f9f; }
`
  document.head.append(style)
}
