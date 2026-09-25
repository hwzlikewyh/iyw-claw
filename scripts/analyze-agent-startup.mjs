import { createReadStream } from "node:fs"
import { basename } from "node:path"
import { createInterface } from "node:readline"

const PERCENTILE_MEDIAN = 0.5
const PERCENTILE_P95 = 0.95
const STAGE_MESSAGE = "[ACP][startup] stage"
const TRANSIENT_OUTCOMES = new Set(["started", "aborted"])

function parseLine(line) {
  try {
    return JSON.parse(line)
  } catch {
    return null
  }
}

function milliseconds(value) {
  if (value == null || value === "") return null
  const number = Number(value)
  return Number.isFinite(number) && number >= 0 ? number : null
}

function addSample(groups, labels, value) {
  const number = milliseconds(value)
  if (number == null) return
  const key = JSON.stringify(labels)
  if (!groups.has(key)) groups.set(key, { ...labels, samples: [] })
  groups.get(key).samples.push(number)
}

function collectStage(state, fields) {
  const labels = {
    version: state.version,
    agent: fields.agent,
    source: fields.source,
    resumed: fields.resumed,
  }
  const trace = `${state.fileIndex}:${fields.startup_trace_id}`
  state.traces.add(trace)
  if (fields.outcome === "error") state.failed.add(trace)
  if (TRANSIENT_OUTCOMES.has(fields.outcome)) return
  addSample(
    state.stages,
    { ...labels, stage: fields.stage, outcome: fields.outcome },
    fields.duration_ms
  )
  if (fields.stage === "selectors_ready" && fields.outcome === "ok") {
    addSample(
      state.ready,
      { ...labels, path: "initialized" },
      fields.since_accepted_ms
    )
  }
  if (
    fields.stage === "prepared_session_lookup" &&
    fields.outcome === "ready"
  ) {
    addSample(
      state.ready,
      { ...labels, path: "prepared_claim" },
      fields.since_accepted_ms
    )
  }
  if (fields.stage === "first_content" && fields.outcome === "received") {
    addSample(state.firstContent, labels, fields.duration_ms)
  }
}

function collectDiagnosticStage(state, fields) {
  if (
    fields?.message === "[ACP][runtime-env] stage" &&
    fields.outcome !== "started"
  ) {
    addSample(
      state.runtimeStages,
      {
        version: state.version,
        agent: fields.agent,
        resumed: fields.resumed,
        stage: fields.stage,
        outcome: fields.outcome,
      },
      fields.duration_ms
    )
  }
  if (fields?.message === "[ACP][turn-timing] stage") {
    addSample(
      state.turnLatency,
      {
        version: state.version,
        agent: fields.agent,
        stage: fields.stage,
      },
      fields.duration_ms
    )
  }
}

async function readLog(path, state) {
  state.version = "unknown"
  let lines = 0
  let invalidLines = 0
  const input = createInterface({
    input: createReadStream(path),
    crlfDelay: Infinity,
  })
  for await (const line of input) {
    if (!line.trim()) continue
    lines += 1
    const entry = parseLine(line)
    if (!entry) {
      invalidLines += 1
      continue
    }
    if (entry.app_version) state.version = entry.app_version
    const fields = entry.fields
    if (fields?.event === "process_context" && fields.app_version) {
      state.version = fields.app_version
    }
    if (fields?.message === STAGE_MESSAGE && fields.startup_trace_id) {
      collectStage(state, fields)
    }
    collectDiagnosticStage(state, fields)
  }
  return { file: basename(path), lines, invalidLines }
}

function percentile(sorted, quantile) {
  return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)]
}

function summarize(groups) {
  return [...groups.values()].map(({ samples, ...labels }) => {
    const sorted = samples.toSorted((left, right) => left - right)
    return {
      ...labels,
      count: sorted.length,
      minMs: sorted[0],
      p50Ms: percentile(sorted, PERCENTILE_MEDIAN),
      p95Ms: percentile(sorted, PERCENTILE_P95),
      maxMs: sorted.at(-1),
    }
  })
}

async function main(paths) {
  if (!paths.length) {
    throw new Error(
      "Usage: node scripts/analyze-agent-startup.mjs <log> [log...]"
    )
  }
  const state = {
    fileIndex: 0,
    version: "unknown",
    traces: new Set(),
    failed: new Set(),
    stages: new Map(),
    ready: new Map(),
    firstContent: new Map(),
    runtimeStages: new Map(),
    turnLatency: new Map(),
  }
  const files = []
  for (const path of paths) {
    files.push(await readLog(path, state))
    state.fileIndex += 1
  }
  console.log(
    JSON.stringify(
      {
        files,
        traceCount: state.traces.size,
        failedTraceCount: state.failed.size,
        readiness: summarize(state.ready),
        firstContentAfterSend: summarize(state.firstContent),
        runtimeEnvironmentStages: summarize(state.runtimeStages),
        turnLatency: summarize(state.turnLatency),
        stages: summarize(state.stages),
      },
      null,
      2
    )
  )
}

main(process.argv.slice(2)).catch((error) => {
  console.error(error.message)
  process.exitCode = 1
})
