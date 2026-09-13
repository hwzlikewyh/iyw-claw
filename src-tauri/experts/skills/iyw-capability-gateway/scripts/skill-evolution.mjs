import fs from "node:fs"
import path from "node:path"
import { parseArgs } from "node:util"
import { pathToFileURL } from "node:url"
import {
  digest, readJson, text, targetPath, snapshot, fingerprint, saveSnapshot,
  withStore, proposalDirectory, loadProposal, saveRecord, requireCurrent, replaceTarget,
} from "./skill-evolution-store.mjs"
import { evaluate } from "./skill-evolution-evaluate.mjs"

export function stage(store, input) {
  const target = text(input.target, "target")
  if (!target.endsWith(".md")) throw new Error("This workflow stages one Markdown instruction file")
  targetPath(store.root, target)
  const content = text(input.content, "content")
  const pattern = text(input.pattern, "pattern")
  const evidence = text(input.evidence, "evidence")
  const files = snapshot(store.root)
  const previous = files.find((file) => file.name === target)
  if (!previous) throw new Error("Target must be an existing Skill instruction file")
  if (previous.data.equals(Buffer.from(content))) throw new Error("Proposal does not change the Skill")
  const baseline = fingerprint(files)
  const candidateFiles = files.map((file) => file.name === target ? { ...file, data: Buffer.from(content) } : file)
  const candidate = fingerprint(candidateFiles)
  const id = digest(`${baseline}:${candidate}`).slice(0, 24)
  const directory = proposalDirectory(store, id)
  if (fs.existsSync(path.join(directory, "record.json"))) return loadProposal(store, id).record
  if (fs.existsSync(directory)) throw new Error("Incomplete staged snapshot; inspect it before retrying")
  fs.mkdirSync(directory)
  saveSnapshot(path.join(directory, "baseline"), files)
  saveSnapshot(path.join(directory, "candidate"), candidateFiles)
  const record = { id, skill: store.root, target, pattern, evidence, baseline, candidate, status: "staged", history: [] }
  saveRecord({ directory, record }, { action: "stage" })
  return record
}

export function apply(store, proposal) {
  const { directory, record } = proposal
  if (record.status !== "evaluated" || !record.evaluation?.accepted) {
    throw new Error("A passing comparison is required before applying the proposal")
  }
  requireCurrent(store, record, record.baseline)
  const files = snapshot(path.join(directory, "candidate"))
  if (fingerprint(files) !== record.candidate) throw new Error("Candidate snapshot changed after evaluation")
  record.status = "applying"
  saveRecord(proposal, { action: "apply_started" })
  replaceTarget(store, record, files.find((file) => file.name === record.target).data)
  record.status = "accepted"
  saveRecord(proposal, { action: "apply" })
  return record
}

export function rollback(store, proposal) {
  const { directory, record } = proposal
  if (!["accepted", "applying", "rolling_back"].includes(record.status)) {
    throw new Error("Only an applied or interrupted proposal can be rolled back")
  }
  const current = fingerprint(snapshot(store.root))
  if (current === record.baseline) {
    record.status = "rolled_back"
    saveRecord(proposal, { action: "rollback_recovered" })
    return record
  }
  requireCurrent(store, record, record.candidate)
  const files = snapshot(path.join(directory, "baseline"))
  if (fingerprint(files) !== record.baseline) throw new Error("Baseline snapshot integrity check failed")
  record.status = "rolling_back"
  saveRecord(proposal, { action: "rollback_started" })
  replaceTarget(store, record, files.find((file) => file.name === record.target).data)
  record.status = "rolled_back"
  saveRecord(proposal, { action: "rollback" })
  return record
}

function list(store) {
  return fs.readdirSync(store.directory).filter((name) => /^[a-f0-9]{24}$/.test(name)).map((id) => {
    const file = path.join(proposalDirectory(store, id), "record.json")
    if (!fs.existsSync(file)) return { id, status: "incomplete" }
    const { record } = loadProposal(store, id)
    const { pattern, evidence, status, history } = record
    return { id, pattern, evidence, status, lastAction: history.at(-1) }
  })
}

function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: { skill: { type: "string" }, input: { type: "string" }, id: { type: "string" }, help: { type: "boolean" } },
  })
  if (values.help) {
    console.log("skill-evolution.mjs <list|stage|show|evaluate|apply|rollback> --skill <directory> [--id <id>] [--input <json>]")
    return
  }
  const [operation] = positionals
  if (positionals.length !== 1 || !["list", "stage", "show", "evaluate", "apply", "rollback"].includes(operation)) {
    throw new Error("Choose list, stage, show, evaluate, apply, or rollback")
  }
  const result = withStore(text(values.skill, "skill"), (store) => {
    if (operation === "list") return list(store)
    if (operation === "stage") return stage(store, readJson(text(values.input, "input")))
    const proposal = loadProposal(store, values.id)
    if (operation === "show") return proposal.record
    if (operation === "evaluate") return evaluate(proposal, readJson(text(values.input, "input")))
    if (operation === "apply") return apply(store, proposal)
    return rollback(store, proposal)
  })
  console.log(JSON.stringify(result, null, 2))
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try { main() } catch (error) {
    console.error(JSON.stringify({ error: error.message }))
    process.exitCode = 1
  }
}
