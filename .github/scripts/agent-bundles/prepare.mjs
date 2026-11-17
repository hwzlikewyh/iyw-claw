import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'
import { mkdir, readFile, realpath, readdir, unlink, writeFile } from 'node:fs/promises'
import { dirname, join, relative, resolve, sep } from 'node:path'
import { probe } from './probe.mjs'

const PACKAGE = '@agentclientprotocol/claude-agent-acp'
const VERSION = '0.108.0'
const SDK_VERSION = '0.3.326'
const INTEGRITY = 'sha512-Y0HI3CB0DfhpWItCn2RE3xgpen001V2U/bHm3G6ZVKXsElF5N4uYyjYlmgNzZBKcBtZK0JqiE96NMaeVXtgJjw=='
const REGISTRY = 'https://registry.npmjs.org'
const ROOT = resolve('.agent-bundle-work')
const PREFIX = join(ROOT, 'prefix')
const BRIDGE = resolve('src-tauri/src/acp/side_question/claude-bootstrap.mjs')
const OUTPUT = resolve('artifacts/managed-yuanshan')
const target = { win32: 'windows', darwin: 'macos', linux: 'linux' }[process.platform]
const arch = { x64: 'x86_64', arm64: 'aarch64' }[process.arch]
const json = async (path) => JSON.parse(await readFile(path, 'utf8'))

function run(command, args, env) {
  const result = spawnSync(command, args, {
    env, cwd: ROOT, windowsHide: true, encoding: 'utf8',
    timeout: 900_000, maxBuffer: 8 * 1024 * 1024,
  })
  if (result.error || result.status !== 0) {
    throw new Error(`Command failed: ${result.error?.message ?? result.stderr?.slice(-4096)}`)
  }
  return result.stdout.trim()
}

async function isolatedEnvironment() {
  for (const name of ['home', 'temp', 'appdata']) await mkdir(join(ROOT, name), { recursive: true })
  const keep = ['PATH', 'Path', 'SystemRoot', 'WINDIR', 'COMSPEC', 'PATHEXT', 'NUMBER_OF_PROCESSORS']
  const env = Object.fromEntries(keep.filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
  return { ...env, HOME: join(ROOT, 'home'), USERPROFILE: join(ROOT, 'home'),
    APPDATA: join(ROOT, 'appdata'), LOCALAPPDATA: join(ROOT, 'appdata'),
    TMPDIR: join(ROOT, 'temp'), TEMP: join(ROOT, 'temp'), TMP: join(ROOT, 'temp'),
    npm_config_cache: join(ROOT, 'npm-cache'), npm_config_userconfig: join(ROOT, 'empty.npmrc'),
    CLAUDE_AGENT_ACP_PROTOCOL: 'v1', NO_COLOR: '1' }
}

async function install(env) {
  const response = await fetch(`${REGISTRY}/${PACKAGE}/${VERSION}`, { signal: AbortSignal.timeout(30_000) })
  if (!response.ok) throw new Error(`Registry metadata HTTP ${response.status}`)
  const meta = await response.json()
  if (meta.version !== VERSION || meta.dist.integrity !== INTEGRITY) throw new Error('Official npm identity changed')
  await mkdir(PREFIX, { recursive: true })
  await writeFile(join(ROOT, 'empty.npmrc'), '')
  await writeFile(join(PREFIX, 'package.json'), JSON.stringify({
    name: 'iyw-managed-agent-runtime', private: true, version: '1.0.0', dependencies: { [PACKAGE]: VERSION },
  }))
  const npmCli = process.platform === 'win32'
    ? join(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js')
    : resolve(dirname(process.execPath), '../lib/node_modules/npm/bin/npm-cli.js')
  run(process.execPath, [npmCli, 'install', '--prefix', PREFIX, '--no-audit', '--no-fund',
    '--save-exact', '--include=optional', '--registry', REGISTRY, `${PACKAGE}@${VERSION}`], env)
  const lock = await json(join(PREFIX, 'package-lock.json'))
  const installed = lock.packages[`node_modules/${PACKAGE}`]
  if (installed?.version !== VERSION || installed.integrity !== INTEGRITY) throw new Error('Installed npm identity mismatch')
}

async function normalizeBins() {
  const binDir = join(PREFIX, 'node_modules/.bin')
  for (const file of await readdir(binDir, { withFileTypes: true })) {
    if (!file.isSymbolicLink()) continue
    const path = join(binDir, file.name)
    const actual = await realpath(path)
    if (!actual.startsWith(PREFIX + sep)) throw new Error('Bin link escaped package')
    const destination = relative(binDir, actual).replaceAll('\\', '/')
    if (!/^[a-zA-Z0-9_@./-]+$/.test(destination)) throw new Error('Unsafe npm bin path')
    await unlink(path)
    await writeFile(path, `#!/bin/sh\nexec node "$(dirname "$0")/${destination}" "$@"\n`, { mode: 0o755 })
  }
}

function manifest() {
  return { schemaVersion: 1, registryId: 'claude-acp', version: VERSION, deliveryKind: 'npm', target, arch,
    entrypoints: { 'claude-agent-acp': `node_modules/.bin/claude-agent-acp${target === 'windows' ? '.cmd' : ''}` },
    components: [{ componentKey: 'claude-acp', packageName: PACKAGE, packageVersion: VERSION, registryIntegrity: INTEGRITY }],
    uvConstraints: {}, uvPrerelease: '', runtimeRequirements: { node: '24.0.0' }, createdBy: 'managed-component-sync' }
}

async function verify(env) {
  const entrypoint = join(PREFIX, `node_modules/${PACKAGE}/dist/index.js`)
  const require = createRequire(entrypoint)
  const sdkRequire = createRequire(require.resolve('@anthropic-ai/claude-agent-sdk'))
  const sdkPlatform = `@anthropic-ai/claude-agent-sdk-${process.platform}-${process.arch}`
  const cli = sdkRequire.resolve(`${sdkPlatform}/claude${target === 'windows' ? '.exe' : ''}`)
  const sdk = await json(join(dirname(require.resolve('@anthropic-ai/claude-agent-sdk')), 'package.json'))
  if (sdk.version !== SDK_VERSION) throw new Error('Native SDK version mismatch')
  const cliVersion = run(cli, ['--version'], env)
  const acpVersion = run(process.execPath, [entrypoint, '--version'], env)
  if (acpVersion !== VERSION) throw new Error('ACP runtime version mismatch')
  const result = await probe([BRIDGE, entrypoint], { cwd: ROOT, env })
  return { ...result, cliVersion, acpVersion, nodeVersion: process.versions.node, sdkVersion: sdk.version,
    sdkPlatform, target, arch, sourceCommit: process.env.GITHUB_SHA ?? 'local',
    bridgeSha256: createHash('sha256').update(await readFile(BRIDGE)).digest('hex') }
}

if (!target || !arch || process.versions.node.split('.')[0] !== '24') throw new Error('Native Node 24 runner required')
if (process.env.EXPECTED_TARGET !== target || process.env.EXPECTED_ARCH !== arch) throw new Error('Runner platform mismatch')
await mkdir(OUTPUT, { recursive: true })
const env = await isolatedEnvironment()
if (process.argv.includes('--verify-only')) {
  await verify(env)
} else {
  await install(env)
  await normalizeBins()
  const result = await verify(env)
  await writeFile(join(PREFIX, 'iyw-agent-bundle.json'), JSON.stringify(manifest()) + '\n')
  await writeFile(join(OUTPUT, 'verification.json'), JSON.stringify(result, null, 2) + '\n')
  console.log(JSON.stringify(result))
}
