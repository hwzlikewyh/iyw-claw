(() => {
  const engine = globalThis.__codegAgent
  let generation = ""
  let aliases = new Map()
  const fail = (message) => { throw new Error(message) }
  function snapshot(args, epoch, prefix) {
    const value = engine.snapshot({ maxChars: 100000, epoch })
    generation = value.generation
    aliases = new Map()
    value.tree = value.tree.replace(/\[ref=(e\d+)\]/g, (_, ref) => {
      const alias = `e${prefix.padStart(20, "0")}${ref.slice(1)}`
      aliases.set(alias, ref)
      return `[ref=${alias}]`
    })
    if (args.includes("--selector")) fail("Scoped snapshots are unavailable in the lightweight browser")
    const depthAt = args.indexOf("--depth")
    if (depthAt >= 0) {
      const depth = Number(args[depthAt + 1])
      value.tree = value.tree.split("\n").filter((line) => line.search(/\S/) < depth * 2).join("\n")
    }
    return { snapshot: value.tree, url: value.url, title: value.title, generation, truncated: value.truncated }
  }
  function element(selector, epoch) {
    if (/^@?e\d+$/.test(selector)) {
      if (!generation.endsWith(`.${epoch}`)) fail("STALE_REF: take a fresh browser snapshot")
      const ref = aliases.get(selector.replace(/^@/, ""))
      const found = ref && engine.elementForRef(generation, ref)
      if (!found) fail("STALE_REF: take a fresh browser snapshot")
      return found
    }
    const found = document.querySelectorAll(selector)
    if (found.length !== 1) fail(`Expected one matching element, found ${found.length}`)
    return found[0]
  }
  function act(args, epoch) {
    const [command, selector, text] = args
    const target = element(selector, epoch)
    if (/^@?e\d+$/.test(selector)) {
      const kind = command === "fill" || command === "type" ? "type" : command === "dblclick" ? "click" : command
      const content = command === "type" ? `${target.value ?? target.textContent ?? ""}${text ?? ""}` : text ?? ""
      const request = kind === "type" ? { kind, text: content } : { kind, count: command === "dblclick" ? 2 : 1 }
      const answer = engine.act(generation, aliases.get(selector.replace(/^@/, "")), request)
      if (!answer.ok) fail(`${answer.error === "stale" ? "STALE_REF: " : ""}${answer.detail}`)
      return answer
    }
    if (["click", "dblclick", "hover"].includes(command)) pointer(args, epoch)
    target.focus()
    if (command === "click" || command === "dblclick") {
      target.click()
      if (command === "dblclick") target.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))
    } else if (command === "hover") {
      target.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }))
    } else if (command === "fill" || command === "type") {
      const content = command === "type" ? `${target.value ?? target.textContent ?? ""}${text ?? ""}` : text ?? ""
      if (target.isContentEditable) target.textContent = content
      else {
        const prototype = target instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
        const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set
        if (!setter || target.disabled || target.readOnly) fail("The element is not editable")
        setter.call(target, content)
      }
      target.dispatchEvent(new Event("input", { bubbles: true }))
      target.dispatchEvent(new Event("change", { bubbles: true }))
    }
    return { url: location.href, fidelity: "synthetic" }
  }
  function pointer(args, epoch) {
    const target = element(args[1], epoch)
    target.scrollIntoView({ block: "center", inline: "nearest" })
    const rect = target.getBoundingClientRect()
    const x = rect.left + rect.width / 2
    const y = rect.top + rect.height / 2
    const cover = document.elementFromPoint(x, y)
    if (!rect.width || !rect.height || x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) fail("The element is not visible")
    if (cover && cover !== target && !target.contains(cover)) fail("The element is covered")
    if (target.disabled) fail("The element is disabled")
    return { x, y, count: args[0] === "dblclick" ? 2 : 1, move: args[0] === "hover", epoch }
  }
  function press(args, epoch) {
    if (!generation.endsWith(`.${epoch}`)) generation = engine.snapshot({ epoch }).generation
    const value = engine.act(generation, null, { kind: "press", key: args[1] })
    if (!value.ok) fail(value.detail)
    return value
  }
  function inspect(args, epoch) {
    const target = element(args[1], epoch)
    const type = args[2] || "text"
    if (type === "text") return { text: target.innerText || target.textContent || "" }
    if (type === "value") return { value: target.value ?? null }
    if (type === "html") return { html: target.outerHTML }
    return { attribute: target.getAttribute(type) }
  }
  function query(args, epoch) {
    if (args[1] === "url") return { url: location.href }
    if (args[1] === "title") return { title: document.title }
    if (args[1] === "text" || args[1] === "value" || args[1] === "html") return inspect([args[0], args[2] || "body", args[1]], epoch)
    return inspect(args, epoch)
  }
  function checkbox(args, epoch) {
    const target = element(args[1], epoch)
    if (!(target instanceof HTMLInputElement) || !["checkbox", "radio"].includes(target.type)) fail("The element is not a checkbox")
    if (target.checked !== (args[0] === "check")) target.click()
    return { checked: target.checked }
  }
  function select(args, epoch) {
    const target = element(args[1], epoch)
    if (!(target instanceof HTMLSelectElement)) fail("The element is not a select")
    const wanted = args.slice(2)
    let count = 0
    for (const option of target.options) {
      option.selected = wanted.includes(option.value) || wanted.includes(option.label)
      if (option.selected) count++
    }
    if (!count) fail("No matching option")
    target.dispatchEvent(new Event("input", { bubbles: true }))
    target.dispatchEvent(new Event("change", { bubbles: true }))
    return { values: Array.from(target.selectedOptions, (option) => option.value) }
  }
  function storage(args) {
    const store = args[1] === "session" ? sessionStorage : localStorage
    if (args[2] === "set") store.setItem(args[3], args[4] ?? "")
    if (args[2] === "clear") store.clear()
    if (args[2] === "remove") store.removeItem(args[3])
    return { storage: Object.fromEntries(Array.from({ length: store.length }, (_, index) => { const key = store.key(index); return [key, store.getItem(key)] })) }
  }
  function find(args, epoch, prefix) {
    const [, kind, name, action, text] = args
    if (!["role", "text"].includes(kind)) fail(`Unsupported lightweight browser locator: ${kind}`)
    const value = snapshot([], epoch, prefix)
    const lines = value.snapshot.split("\n")
    const namedAt = args.indexOf("--name")
    const label = namedAt < 0 ? null : args[namedAt + 1]
    const matches = lines.filter((line) => line.includes(name) && (!label || line.includes(JSON.stringify(label))) && (kind !== "role" || line.trimStart().startsWith(`- ${name}`)))
    const refs = matches.map((line) => /\[ref=(e\d+)\]/.exec(line)?.[1]).filter(Boolean)
    if (refs.length !== 1) fail(`Expected one matching element, found ${refs.length}`)
    if (!["click", "dblclick", "hover", "fill", "type"].includes(action)) fail(`Unsupported lightweight browser locator action: ${action}`)
    return act([action, `@${refs[0]}`, text], epoch)
  }
  function execute(args, epoch, prefix) {
    switch (args[0]) {
      case "snapshot": case "a11y": return snapshot(args, epoch, prefix)
      case "read": {
        if (args.includes("--filter")) fail("Filtered reads are unavailable in the lightweight browser")
        const text = args.includes("--raw") ? document.documentElement.outerHTML : args.includes("--outline") ? Array.from(document.querySelectorAll("h1,h2,h3,h4,h5,h6"), (element) => `${element.tagName}: ${element.textContent}`).join("\n") : document.body?.innerText ?? ""
        return { text, url: location.href, title: document.title }
      }
      case "pointer": return pointer(args.slice(1), epoch)
      case "click": case "dblclick": case "hover": case "fill": case "type": return act(args, epoch)
      case "press": return press(args, epoch)
      case "scroll": { const delta = Number(args[2]) || 600; const sign = ["up", "left"].includes(args[1]) ? -1 : 1; const horizontal = ["left", "right"].includes(args[1]); window.scrollBy(horizontal ? sign * delta : 0, horizontal ? 0 : sign * delta); return { x: scrollX, y: scrollY } }
      case "exists": return { exists: /^@?e\d+$/.test(args[1]) ? !!element(args[1], epoch) : !!document.querySelector(args[1]) }
      case "get": return query(args, epoch)
      case "inspect": return inspect(args, epoch)
      case "check": case "uncheck": return checkbox(args, epoch)
      case "select": return select(args, epoch)
      case "focus": element(args[1], epoch).focus(); return { focused: true }
      case "scrollintoview": element(args[1], epoch).scrollIntoView({ block: "center" }); return { done: true }
      case "storage": return storage(args)
      case "find": return find(args, epoch, prefix)
      case "eval": return (0, eval)(args[1] || "null")
      case "pushstate": history.pushState({}, "", args[1]); return { url: location.href }
      case "is": { const target = element(args[2], epoch); return { value: args[1] === "checked" ? !!target.checked : args[1] === "enabled" ? !target.disabled : !!target.getClientRects().length } }
      default: fail(`Unsupported lightweight browser command: ${args[0]}`)
    }
  }
  globalThis.__iywNativeBrowser = { execute }
})()
