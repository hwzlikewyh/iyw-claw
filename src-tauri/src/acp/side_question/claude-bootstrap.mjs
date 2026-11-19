// Application-owned extension; imports, but never patches, the managed package.
import { createRequire } from "node:module"
import { dirname, join } from "node:path"
import { pathToFileURL } from "node:url"
import { appendFileSync, mkdirSync, readFileSync } from "node:fs"

const entrypoint = process.argv[2]
if (!entrypoint) throw new Error("Missing managed ACP entrypoint")
process.argv.splice(1, 2, entrypoint)
const require = createRequire(entrypoint)
const load = (name) => import(pathToFileURL(require.resolve(name)).href)
const loadInternal = (name) => import(pathToFileURL(join(dirname(entrypoint), name)).href)
const version = JSON.parse(
  readFileSync(join(dirname(entrypoint), "../package.json"), "utf8")
).version
const usesProtocolRouter = ["0.107.3", "0.108.0"].includes(version)

async function runLatestAcp(logger, onAgent) {
  const [protocol, server, transport, v1, v2] = await Promise.all([
    load("@agentclientprotocol/sdk/experimental/v2"),
    loadInternal("serve.js"),
    loadInternal("utils.js"),
    loadInternal("v1/acp-agent.js"),
    loadInternal("v2/agent.js"),
  ])
  const router = protocol.agentProtocolRouter()
  const protocols = server.acpProtocolsFromEnv(
    process.env.CLAUDE_AGENT_ACP_PROTOCOL
  )
  if (protocols !== "v2") router.withV1(v1.v1AgentApp(logger, onAgent))
  if (protocols !== "v1") router.withV2(v2.v2AgentApp(logger, onAgent))
  return router.connect(
    protocol.ndJsonStream(
      transport.nodeToWebWritable(process.stdout),
      transport.nodeToWebReadable(process.stdin)
    )
  )
}

// 只为已核验的受管版本安装原生侧问扩展。
if (
  !["0.73.0", "0.98.0", "0.107.3", "0.108.0"].includes(version) ||
  (usesProtocolRouter && process.env.CLAUDE_AGENT_ACP_PROTOCOL === "v2") ||
  process.argv.some((arg) => ["--cli", "--version", "-v"].includes(arg))
) {
  // Preserve ordinary sessions on newer packages; absence of our RPC is explicit.
  await import(pathToFileURL(entrypoint).href)
} else {
  const { AgentApp } = await load(
    version === "0.73.0"
      ? "@agentclientprotocol/sdk"
      : "@agentclientprotocol/sdk/unstable"
  )
  let runAcp
  if (usesProtocolRouter) {
    runAcp = async (logger) => ({
      connection: await runLatestAcp(logger, (created) => {
        agent = created
      }),
    })
  } else {
    const native = await loadInternal("acp-agent.js")
    runAcp = native.runAcp
  }
  const sdk = await load("@anthropic-ai/claude-agent-sdk")
  if (
    typeof AgentApp?.prototype?.connect !== "function" ||
    typeof runAcp !== "function" ||
    typeof sdk.resolveSettings !== "function"
  ) {
    throw new Error(
      "Managed native ACP shape is incompatible with side-question bootstrap"
    )
  }
  // Native errors can echo prompts or credentials. Log only stable diagnostic
  // categories, while the request itself still receives the complete error.
  const describeError = (error) => {
    const message = String(error?.message ?? "")
    const reason = /timeout|timed out/i.test(message)
      ? "timeout"
      : /abort|cancel/i.test(message)
        ? "cancelled"
        : /auth|credential|unauthorized|forbidden/i.test(message)
          ? "authorization"
          : /rate.?limit|too many requests/i.test(message)
            ? "rate-limit"
            : /network|socket|connect|fetch/i.test(message)
              ? "transport"
              : /exceeds size limit/i.test(message)
                ? "answer-size-limit"
                : /no answer/i.test(message)
                  ? "empty-answer"
                  : /tool/i.test(message)
                    ? "tool-unavailable"
                    : "native-error"
    const status = Number.isInteger(error?.status) ? error.status : null
    return JSON.stringify({ reason, status })
  }
  try {
    const policy = await sdk.resolveSettings({ settingSources: [] })
    for (const [key, value] of Object.entries(policy.effective.env ?? {})) {
      process.env[key] = value
    }
  } catch (error) {
    // 与新版原生入口一致，策略读取失败不阻止 ACP 启动；日志仅保留脱敏分类。
    console.error(
      `[side-question] stage=managed_policy detail=${describeError(error)}`
    )
  }
  console.log = console.info = console.warn = console.debug = console.error
  process.on("unhandledRejection", (error) => {
    console.error(
      `[side-question] stage=unhandled_rejection detail=${describeError(error)}`
    )
  })
  // Keep the managed entrypoint's optional file logger for upstream ACP events.
  const logDirectory = process.env.CLAUDE_AGENT_LOGS
  const logger = logDirectory
    ? (() => {
        mkdirSync(logDirectory, { recursive: true })
        const logFile = join(logDirectory, "agent.log")
        const log = (...args) => {
          const rendered = args
            .map((arg) =>
              arg instanceof Error ? (arg.stack ?? arg.message) : String(arg)
            )
            .join(" ")
          appendFileSync(
            logFile,
            `${new Date().toISOString()} pid=${process.pid} ${rendered}\n`
          )
        }
        return {
          log,
          error: (...args) => {
            console.error(...args)
            log(...args)
          },
        }
      })()
    : undefined
  let agent
  let closing = false
  let installed = 0
  let resolveInstalled
  const installedConnection = new Promise((resolve) => {
    resolveInstalled = resolve
  })
  const pending = new Map()
  const seen = new Set()
  const remember = (id) => {
    seen.add(id)
    if (seen.size > 256) seen.delete(seen.values().next().value)
  }
  const parser = {
    parse(p) {
      if (
        !p ||
        typeof p.sessionId !== "string" ||
        !p.sessionId ||
        p.sessionId.length > 256 ||
        !["capabilities", "ask", "cancel"].includes(p.action)
      ) {
        throw new Error("Invalid side-question request")
      }
      if (
        p.action !== "capabilities" &&
        (typeof p.requestId !== "string" ||
          !/^[A-Za-z0-9-]{1,100}$/.test(p.requestId))
      ) {
        throw new Error("Invalid side-question request id")
      }
      if (
        p.action === "ask" &&
        (typeof p.question !== "string" ||
          !p.question.trim() ||
          Buffer.byteLength(p.question) > 32768)
      ) {
        throw new Error("Invalid side question")
      }
      return p
    },
  }
  const handler = async ({ params: p, signal }) => {
    if (closing) throw new Error("Side-question session is closing")
    const session = agent?.sessions?.[p.sessionId]
    if (!session || session.queryClosed)
      throw new Error("Side-question session is not available")
    const supported = typeof session.query?.askSideQuestion === "function"
    if (p.action === "capabilities")
      return { supported, mode: "native-context-only" }
    if (!supported)
      throw new Error("This native SDK does not expose side questions")
    const key = `${p.sessionId}:${p.requestId}`
    if (p.action === "cancel") {
      remember(key) // Fence a cancel that crosses the ask on the transport.
      pending.get(p.sessionId)?.key === key &&
        pending.get(p.sessionId).controller.abort()
      return { cancelled: true }
    }
    if (seen.has(key))
      throw new Error("Side-question request was already used or cancelled")
    if (pending.has(p.sessionId))
      throw new Error("A side question is already running")
    remember(key)
    const controller = new AbortController()
    pending.set(p.sessionId, { key, controller })
    const abort = () => controller.abort()
    signal.addEventListener("abort", abort, { once: true })
    if (signal.aborted) abort()
    const timeout = setTimeout(abort, 150000)
    const startedAt = Date.now()
    console.error(`[side-question] request=${p.requestId} state=started`)
    try {
      if (controller.signal.aborted) return { cancelled: true }
      const result = await session.query.askSideQuestion(p.question, {
        signal: controller.signal,
      })
      if (controller.signal.aborted) return { cancelled: true }
      if (!result || typeof result.response !== "string")
        throw new Error("Native side question returned no answer")
      if (Buffer.byteLength(result.response) > 262144)
        throw new Error("Native side answer exceeds size limit")
      console.error(
        `[side-question] request=${p.requestId} state=completed elapsed_ms=${Date.now() - startedAt}`
      )
      return { response: result.response, synthetic: result.synthetic ?? false }
    } catch (error) {
      console.error(
        `[side-question] request=${p.requestId} state=${controller.signal.aborted ? "cancelled" : "failed"} elapsed_ms=${Date.now() - startedAt} detail=${describeError(error)}`
      )
      if (controller.signal.aborted) return { cancelled: true }
      throw error
    } finally {
      clearTimeout(timeout)
      signal.removeEventListener("abort", abort)
      if (pending.get(p.sessionId)?.key === key) pending.delete(p.sessionId)
    }
  }
  const inputParser = {
    parse(params) {
      if (
        typeof params?.sessionId !== "string" ||
        typeof params.interrupt !== "boolean"
      ) throw new Error("Invalid input boundary request")
      return params
    },
  }
  const waitForInput = async ({ params, signal }) => {
    const id = params.sessionId
    const sessions = agent?.sessions ?? {}
    const session = Object.hasOwn(sessions, id) ? sessions[id] : null
    let interrupt = params.interrupt
    while (!signal.aborted && session && !session.queryClosed && agent.sessions[id] === session) {
      const active = session.activeTurn
      const turn = active && !active.settled ? active : session.turnQueue?.find((next) => !next.settled)
      if (!turn) return { ready: true }
      if (typeof turn.completion?.then !== "function") return { supported: false }
      if (interrupt && !session.cancelled) await agent.cancel({ sessionId: id })
      interrupt = false
      await turn.completion
    }
    throw new Error("Input session closed")
  }
  // Register on the original builder; leave every upstream handler intact.
  const connect = AgentApp.prototype.connect
  let connection
  try {
    AgentApp.prototype.connect = function (...args) {
      installed++
      if (installed !== 1)
        throw new Error(
          "Unexpected ACP connection during side-question bootstrap"
        )
      this.onRequest("_iyw/input_boundary", inputParser, waitForInput)
      this.onRequest("_iyw/side_question", parser, handler)
      const result = connect.apply(this, args)
      resolveInstalled()
      return result
    }
    const runtime = await runAcp(logger)
    connection = runtime.connection
    agent = runtime.agent ?? agent
    await installedConnection
  } finally {
    AgentApp.prototype.connect = connect
  }
  if (installed !== 1)
    throw new Error("Native side-question extension was not installed")
  const shutdown = async () => {
    if (closing) return
    closing = true
    for (const { controller } of pending.values()) controller.abort()
    try {
      await agent?.dispose()
    } finally {
      process.exit(0)
    }
  }
  connection.closed.then(shutdown, shutdown)
  process.on("SIGTERM", shutdown)
  process.on("SIGINT", shutdown)
  process.stdin.resume()
}
