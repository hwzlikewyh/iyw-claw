import fs from "node:fs"
import path from "node:path"
import os from "node:os"
import { spawnSync } from "node:child_process"
import { digest, fingerprint, snapshot, saveSnapshot, saveRecord, text } from "./skill-evolution-store.mjs"

const CASE_TIMEOUT_MS = 60_000
const MAX_OUTPUT_BYTES = 64 * 1024
const MAX_CASES = 8

export function evaluate(proposal, input) {
  if (proposal.record.status !== "staged") throw new Error("Only staged proposals can be evaluated")
  validateInput(input)
  const { directory, record } = proposal
  const baseline = snapshot(path.join(directory, "baseline"))
  const candidate = snapshot(path.join(directory, "candidate"))
  if (fingerprint(baseline) !== record.baseline || fingerprint(candidate) !== record.candidate) {
    throw new Error("Snapshot integrity check failed")
  }
  const evaluation = runComparison(input, { baseline, candidate })
  record.evaluation = evaluation
  record.status = evaluation.accepted ? "evaluated" : "rejected"
  saveRecord(proposal, { action: "evaluate", accepted: evaluation.accepted })
  return record
}

function validateInput(input) {
  const { command, cases, rubric } = input
  text(rubric, "rubric")
  if (!Array.isArray(command) || !command.length || command.length > 16) {
    throw new Error("command must be an executable and argument array; shell strings are unsupported")
  }
  command.forEach((argument) => text(argument, "command argument"))
  if (!path.isAbsolute(command[0])) throw new Error("Use an absolute evaluator executable")
  if (!Array.isArray(cases) || cases.length < 2 || cases.length > MAX_CASES) {
    throw new Error(`Supply 2-${MAX_CASES} identical baseline/candidate cases, including a regression case`)
  }
  const ids = new Set()
  for (const item of cases) {
    text(item.id, "case id")
    text(item.input, "case input")
    if (ids.has(item.id)) throw new Error("Case IDs must be unique")
    ids.add(item.id)
  }
}

function runComparison(input, variants) {
  const rows = []
  for (const item of input.cases) {
    const baseline = runCase(input, item, variants.baseline)
    const candidate = runCase(input, item, variants.candidate)
    rows.push({ id: item.id, inputDigest: digest(item.input), baseline, candidate })
  }
  const noRegression = rows.every(({ baseline, candidate }) =>
    baseline.valid && candidate.valid && candidate.passed && candidate.score >= baseline.score)
  const improved = rows.some(({ baseline, candidate }) => candidate.score > baseline.score)
  return {
    command: input.command, rubric: input.rubric, cases: rows,
    accepted: noRegression && improved,
    rule: "All candidate cases pass, no case regresses, and at least one score strictly improves",
  }
}

function runCase(input, item, files) {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "iyw-skill-eval-"))
  try {
    saveSnapshot(scratch, files)
    const result = spawnSync(input.command[0], input.command.slice(1), {
      cwd: scratch, shell: false, windowsHide: true, encoding: "utf8",
      timeout: CASE_TIMEOUT_MS, maxBuffer: MAX_OUTPUT_BYTES,
      env: {
        ...process.env, IYW_EVAL_SKILL_DIR: scratch,
        IYW_EVAL_CASE: JSON.stringify(item), IYW_EVAL_RUBRIC: input.rubric,
      },
    })
    return decodeResult(result)
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true })
  }
}

function decodeResult(result) {
  if (result.error || result.status !== 0) {
    return { valid: false, score: 0, passed: false, evidence: "Evaluator failed or timed out", exitCode: result.status }
  }
  try {
    const value = JSON.parse(result.stdout)
    if (!Number.isFinite(value.score) || value.score < 0 || value.score > 1) throw new Error("score")
    if (typeof value.passed !== "boolean") throw new Error("passed")
    text(value.evidence, "evidence")
    if (value.evidence.length > MAX_OUTPUT_BYTES) throw new Error("evidence too large")
    return { valid: true, score: value.score, passed: value.passed, evidence: value.evidence }
  } catch {
    return { valid: false, score: 0, passed: false, evidence: "Evaluator did not return valid score/passed/evidence JSON" }
  }
}
