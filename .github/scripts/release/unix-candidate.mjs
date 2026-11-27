import { readFileSync } from "node:fs"
import { join } from "node:path"
import { downloadReleaseFile, requireReleaseSource, uploadDraftFile } from "./github-assets.mjs"
import { verifyMinisign } from "./verify-minisign.mjs"

const { VERSION, SOURCE_REF, RELEASE_ID, ARCH, RUNNER_TEMP, GITHUB_REPOSITORY } = process.env
if (!/^\d+\.\d+\.\d+$/.test(VERSION) || !/^[a-f0-9]{40}$/.test(SOURCE_REF) || !/^[0-9]+$/.test(RELEASE_ID)) {
  throw new Error("Expected a version, immutable application source and draft release ID")
}
const context = { repo: GITHUB_REPOSITORY, releaseId: RELEASE_ID, tag: `v${VERSION}`, sourceRef: SOURCE_REF }
const release = requireReleaseSource(context)
if (!release.prerelease || release.target_commitish !== SOURCE_REF) throw new Error("Expected a candidate draft")
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"))
const key = Buffer.from(config.plugins.updater.pubkey, "base64").toString("utf8").split(/\r?\n/).find(line => line.startsWith("RW"))
const base = `iyw-claw_${VERSION}_${ARCH}`

async function verify(path) {
  await verifyMinisign(path, readFileSync(`${path}.sig`, "utf8"), key)
}
if (process.argv[2] === "linux" && ["amd64", "arm64"].includes(ARCH)) {
  for (const name of [`${base}.AppImage`, `${base}.AppImage.sig`]) {
    const asset = release.assets.find(asset => asset.name === name)
    if (!asset) throw new Error(`Missing ${name} from candidate draft`)
    await downloadReleaseFile(context, asset, RUNNER_TEMP)
  }
  await verify(join(RUNNER_TEMP, `${base}.AppImage`))
} else if (process.argv[2] === "macos" && ["x64", "aarch64"].includes(ARCH)) {
  const directory = join(RUNNER_TEMP, "signed")
  await verify(join(directory, `${base}.app.tar.gz`))
  for (const suffix of [".app.tar.gz", ".app.tar.gz.sig", ".dmg"]) {
    await uploadDraftFile(context, join(directory, base + suffix))
  }
} else {
  throw new Error("Unexpected platform or architecture")
}
