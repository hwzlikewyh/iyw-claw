// 独立预览的演示时间线；仅模拟流式事件，不连接产品运行时。
const playback = { running: false, finished: false, elapsed: 0, last: 0,
  frame: null, index: -1, mode: null, events: new Map() }
const scenes = [
  { at: 0, stage: 0, mode: "normal", state: "model-wait", name: "初始化对话", title: "正在初始化对话环境", sub: "普通对话 · 准备当前会话" },
  { at: 1900, stage: 0, mode: "normal", state: "model-wait", name: "等待模型", title: "已发起对话，等待模型响应", sub: "请求已发送 · 收到响应后继续更新" },
  { at: 4000, stage: 1, mode: "normal", state: "thinking", name: "思考中", title: "正在分析调用链", sub: "正在接收思考内容" },
  { at: 8200, stage: 2, mode: "normal", state: "working", name: "工具执行", title: "正在核对窗口内判断", sub: "工具执行中 · 读取响应结构与返回类型" },
  { at: 12400, stage: 3, mode: "normal", state: "thinking", name: "继续输出", title: "正在核对修复方案", sub: "正在接收新的思考内容" },
  { at: 18900, stage: 4, mode: "normal", state: "complete", name: "本轮完成", title: "本轮回复已完成", sub: "修复与验证结果已返回" },
  { at: 21600, stage: 2, mode: "goal", state: "working", name: "目标推进", title: "正在推进当前目标", sub: "工具执行中 · 读取响应结构与返回类型" },
  { at: 25000, stage: 3, mode: "goal", state: "thinking", name: "目标输出", title: "正在核对修复方案", sub: "正在接收新的思考内容" },
  { at: 30800, stage: 4, mode: "goal", state: "complete", name: "目标完成", title: "目标已完成", sub: "修复与验证结果已返回" },
  { at: 33200, stage: 5, mode: "loop", state: "waiting", name: "循环等待", title: "等待下次检查", sub: "下次计划 14:10 · 收到调度回执后继续" },
  { at: 37200, stage: 6, mode: "loop", state: "working", name: "收到新活动", title: "会话正在检查构建结果", sub: "已收到工具执行事件 · 读取最新构建日志" },
  { at: 41200, stage: 7, mode: "loop", state: "disconnected", name: "连接断开", title: "连接已断开", sub: "最后收到：读取最新构建日志 · 运行状态尚未确认" },
  { at: 44500, stage: 8, mode: "loop", state: "waiting", name: "连接恢复", title: "连接已恢复，等待下次检查", sub: "已同步调度状态 · 下次计划 14:15" },
]
const narrative = {
  firstThought: "先从窗口判断的入口开始，核对调用参数和响应体。\n\n返回类型显示它是异步调用，需要继续确认调用方是否等待了实际返回值，再判断问题出在解析过程还是条件分支。",
  interim: "已经定位到一个异步调用点，我正在核对它的实际返回值与上层判断分支。",
  secondThought: "调用链与响应结构已核对，问题来自异步返回值被直接当作布尔值使用。修复时只调整相关调用，保留原有错误路径。",
  result: "已确认问题位于 isWithinWindow() 的上层调用：异步返回值在等待解析前，就参与了布尔判断。\n\n我已补齐 await，并核对相关调用点和空值分支。修复结果和验证记录已整理，前面的分析和执行记录都会保留。",
  loopThought: "先对比最近的执行状态和制品列表，避免把上一轮日志当作新结果。",
  loopBody: "正在核对最新构建日志和安装包列表，有新的结果会在这里继续更新。",
}
const PLAYBACK_DURATION = 48000
const REVEAL_INTERVAL = 58
const CHARS_PER_CHUNK = 3
const stream = document.createElement("div")
stream.className = "event-stream"
stream.id = "event-stream"
stream.hidden = true
$("thinking").before(stream)

function addEvent(key, type) {
  if (playback.events.has(key)) return playback.events.get(key)
  const element = document.createElement(type === "text" ? "div" : "details")
  element.dataset.eventId = key
  element.dataset.eventType = type
  element.className = type === "text" ? "prose" : type === "reasoning" ? "thinking" : "tool-history ordered-tools"
  if (type === "text") element.innerHTML = '<p style="white-space:pre-wrap"></p>'
  if (type === "reasoning") {
    element.innerHTML = '<summary><svg><use href="#i-think"/></svg><span class="event-label">正在思考</span><span class="thinking-tag">流式输出中</span><svg class="chevron"><use href="#i-arrow"/></svg></summary><div class="thinking-body" tabindex="0" aria-label="思考内容"><p style="white-space:pre-wrap"></p></div>'
  }
  if (type === "tools") {
    element.innerHTML = '<summary><span class="event-label">正在执行工具</span><svg class="chevron"><use href="#i-arrow"/></svg></summary><div class="steps"><div class="step"><span class="check">✓</span><span class="tool-one">读取窗口 API 定义</span></div><div class="step"><span class="check">✓</span><span class="tool-two">定位判断分支与调用位置</span></div><div class="step active"><span class="tiny-spin"></span><span class="tool-current">读取响应结构与返回类型</span></div></div>'
  }
  const record = { element, type, done: false, body: element.querySelector("p"), label: element.querySelector(".event-label") }
  if (type !== "text") element.open = true
  stream.append(element)
  playback.events.set(key, record)
  return record
}

function finishThought(record, content) {
  record.body.textContent = content
  if (record.done) return
  record.done = true
  record.element.open = false
  const seconds = Math.ceil(content.length / CHARS_PER_CHUNK * REVEAL_INTERVAL / 1000)
  record.label.textContent = `思考了 ${seconds} 秒`
  record.element.querySelector(".thinking-tag").textContent = "查看思考"
}

function revealThought(record, content, count) {
  if (record.done) return
  record.body.textContent = content.slice(0, count)
  if (count >= content.length) finishThought(record, content)
  else record.element.querySelector(".thinking-body").scrollTop = record.body.scrollHeight
}

function finishTools(record) {
  if (record.done) return
  record.done = true
  record.label.textContent = "✓ 已完成 3 项操作 · 查看步骤"
  record.element.open = false
  const row = record.element.querySelector(".step.active")
  row.classList.remove("active")
  row.querySelector(".tiny-spin").replaceWith(Object.assign(document.createElement("span"), { className: "check", textContent: "✓" }))
}

function prepareOrderedEvents(index) {
  const stage = scenes[index].stage
  if (scenes[index].mode !== "loop") {
    if (stage >= 1) addEvent("thought-1", "reasoning")
    if (stage >= 2) {
      finishThought(addEvent("thought-1", "reasoning"), narrative.firstThought)
      addEvent("interim", "text").body.textContent = narrative.interim
      addEvent("tools", "tools")
    }
    if (stage >= 3) {
      finishTools(addEvent("tools", "tools"))
      addEvent("thought-2", "reasoning")
    }
    if (stage === 4) {
      finishThought(addEvent("thought-2", "reasoning"), narrative.secondThought)
      const result = addEvent("result", "text")
      result.body.textContent = narrative.result
      result.element.classList.remove("streaming-copy")
    }
  } else if (stage >= 6) {
    finishThought(addEvent("loop-thought", "reasoning"), narrative.loopThought)
    addEvent("loop-body", "text").body.textContent = narrative.loopBody
    const record = addEvent("loop-tools", "tools")
    record.element.querySelector(".tool-one").textContent = "读取构建任务状态"
    record.element.querySelector(".tool-two").textContent = "核对最近执行结果"
    record.element.querySelector(".tool-current").textContent = stage >= 7 ? "读取日志 · 结果待同步" : "读取最新构建日志"
    record.label.textContent = stage >= 7 ? "执行记录 · 结果待同步" : "正在执行工具"
    if (stage >= 7) {
      record.element.querySelector(".step.active")?.classList.remove("active")
      record.element.querySelector(".tiny-spin")?.replaceWith(Object.assign(document.createElement("span"), { textContent: "·" }))
    }
    if (stage === 8) record.element.open = false
  }
}

function enterScene(index) {
  const scene = scenes[index]
  playback.index = index
  playback.entering = true
  if (demo.mode !== scene.mode) chooseMode(scene.mode)
  if (playback.mode !== scene.mode) {
    stream.replaceChildren()
    playback.events.clear()
    playback.mode = scene.mode
  }
  demo.state = scene.state; demo.seconds = 0; demo.toolSeconds = 0
  $("state").value = scene.state
  render()
  playback.entering = false
  text("activity-title", scene.title); text("activity-sub", scene.sub)
  text("scene-label", `${index + 1} / ${scenes.length} · ${scene.name}`)
  for (const id of ["body-copy", "thinking", "steps", "earlier-tools"]) show(id, false)
  stream.hidden = false
  prepareOrderedEvents(index)
  show("metric-first", scene.stage > 0); show("metric-tools", scene.stage > 1 && scene.mode !== "loop"); show("metric-points", scene.stage > 0)
  text("metric-tools", "3 次工具调用")
  if (scene.stage === 4) text("metric-points", "0.2070 点")
  const scroller = $("thread-scroll"), task = document.querySelector(".task-strip")
  const target = $("live-turn").offsetTop - task.offsetHeight - 55
  scroller.scrollTo({ top: Math.max(0, target), behavior: "instant" })
}

function renderOrderedSnapshot() {
  if (playback.running || playback.entering) return
  const candidate = scenes.findIndex(scene => scene.mode === demo.mode && scene.state === demo.state)
  const fallback = scenes.findIndex(scene => scene.mode === demo.mode && scene.state === "working")
  const index = candidate >= 0 ? candidate : fallback
  stream.replaceChildren(); playback.events.clear(); playback.mode = demo.mode
  for (const id of ["body-copy", "thinking", "steps", "earlier-tools"]) show(id, false)
  stream.hidden = false
  if (demo.state === "model-wait") return
  prepareOrderedEvents(index)
  const stage = scenes[index].stage
  if (stage === 1) revealThought(addEvent("thought-1", "reasoning"), narrative.firstThought, 27)
  if (stage === 3) revealThought(addEvent("thought-2", "reasoning"), narrative.secondThought, 27)
  if (demo.mode === "normal" && demo.state === "complete") text("activity-title", "本轮回复已完成")
}

function revealCopy(sceneTime) {
  const scene = scenes[playback.index], stage = scene.stage
  const count = Math.floor(sceneTime / REVEAL_INTERVAL) * CHARS_PER_CHUNK
  if (stage === 1) revealThought(addEvent("thought-1", "reasoning"), narrative.firstThought, count)
  if (stage === 3) {
    const thought = addEvent("thought-2", "reasoning")
    revealThought(thought, narrative.secondThought, count)
    const delay = Math.ceil(narrative.secondThought.length / CHARS_PER_CHUNK) * REVEAL_INTERVAL
    if (sceneTime >= delay) {
      text("activity-title", "正在整理修复结论")
      text("activity-sub", "正在接收新的正文")
      const result = addEvent("result", "text")
      const length = Math.floor((sceneTime - delay) / REVEAL_INTERVAL) * CHARS_PER_CHUNK
      result.body.textContent = narrative.result.slice(0, length)
      result.element.classList.toggle("streaming-copy", length < narrative.result.length)
    }
  }
  if (scenes[playback.index].state === "working" && sceneTime > 2200) {
    const record = playback.events.get(stage === 2 ? "tools" : "loop-tools")
    record.element.querySelector(".tool-current").textContent = stage === 2 ? "核对空值处理与关联调用点" : "比对最新安装包与更新时间"
    text("activity-sub", "工具执行中 · " + record.element.querySelector(".tool-current").textContent)
  }
  if (stage > 0 && stage < 4) text("metric-points", `${(0.032 + stage * 0.045 + Math.min(count, 100) * 0.0003).toFixed(4)} 点`)
  demo.seconds = Math.floor(sceneTime / 1000); demo.toolSeconds = demo.seconds
  updateClock()
}

function updatePlaybackButton() {
  text("play-demo", playback.running ? "Ⅱ 暂停动画" : playback.finished ? "↻ 再看一遍" : "▶ 播放动画")
  $("play-demo").setAttribute("aria-pressed", String(playback.running))
}

function playbackFrame(now) {
  if (!playback.running) return
  if (!document.hidden && !document.body.classList.contains("page-hidden") && !document.body.classList.contains("motion-off")) playback.elapsed += Math.max(0, Math.min(now - playback.last, 150))
  playback.last = now
  const index = scenes.findLastIndex(scene => scene.at <= playback.elapsed)
  if (index !== playback.index) enterScene(index)
  revealCopy(playback.elapsed - scenes[index].at)
  if (playback.elapsed >= PLAYBACK_DURATION) {
    playback.running = false; playback.finished = true
    text("scene-label", "演示结束 · 可重播"); updatePlaybackButton()
    return
  }
  playback.frame = requestAnimationFrame(playbackFrame)
}

function togglePlayback(restart = false) {
  cancelAnimationFrame(playback.frame)
  if (playback.running && !restart) {
    playback.running = false; document.body.classList.add("motion-off")
    text("motion", "恢复动效"); $("motion").setAttribute("aria-pressed", "true")
    updatePlaybackButton(); return
  }
  if (restart || playback.finished) {
    playback.elapsed = 0; playback.index = -1; playback.finished = false; playback.mode = null
    $("thread-scroll").scrollTop = 0
  }
  playback.running = true; playback.last = performance.now()
  document.body.classList.add("playing"); document.body.classList.remove("motion-off")
  text("motion", "暂停动效"); $("motion").setAttribute("aria-pressed", "false")
  clearInterval(demo.timer); updatePlaybackButton()
  playback.frame = requestAnimationFrame(playbackFrame)
}

function stopForManual(event) {
  if (!event.isTrusted || event.target.closest("#play-demo,#replay-demo,#motion,#references,#design-note,#close-dialog,.reference-dialog")) return
  if (event.type === "click" && !event.target.closest("button,select,input,textarea,summary")) return
  if (playback.running) {
    playback.running = false; cancelAnimationFrame(playback.frame)
    text("scene-label", "手动预览"); updatePlaybackButton()
  }
  if (event.target.closest("#normal-mode,#goal-mode,#loop-mode,#state")) {
    stream.hidden = true; document.body.classList.remove("playing")
    playback.mode = null; playback.finished = true
  }
}

$("play-demo").onclick = () => togglePlayback()
$("replay-demo").onclick = () => togglePlayback(true)
document.addEventListener("click", stopForManual, true)
document.addEventListener("change", stopForManual, true)
document.addEventListener("visibilitychange", () => { playback.last = performance.now() })
matchMedia("(prefers-reduced-motion: reduce)").addEventListener("change", event => {
  if (event.matches && playback.running) togglePlayback()
})
if (!matchMedia("(prefers-reduced-motion: reduce)").matches && !new URLSearchParams(location.search).has("manual")) setTimeout(() => togglePlayback(true), 650)
