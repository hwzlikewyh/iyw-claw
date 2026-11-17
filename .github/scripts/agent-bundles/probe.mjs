import { spawn } from 'node:child_process'
import { createInterface } from 'node:readline'

const TIMEOUT_MS = 120_000
const MAX_DIAGNOSTIC = 4_096

export function probe(args, options) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, args, {
      ...options, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    })
    let stderr = ''
    let initialized = false
    let registered = false
    let failure
    const timer = setTimeout(() => {
      failure = new Error('ACP initialization timed out')
      child.kill()
    }, TIMEOUT_MS)
    child.stderr.on('data', (value) => {
      stderr = (stderr + value).slice(-MAX_DIAGNOSTIC)
    })
    const lines = createInterface({ input: child.stdout })
    child.on('error', (error) => { failure = error })
    child.on('close', (code) => {
      clearTimeout(timer)
      lines.close()
      if (failure || code !== 0 || !initialized || !registered) {
        reject(failure ?? new Error(`ACP probe failed (exit=${code}): ${stderr}`))
      } else resolve({ protocolVersion: 1, sideQuestionRegistered: true })
    })
    lines.on('line', (line) => {
      try {
        const value = JSON.parse(line)
        if (value.id === 1) {
          if (value.result?.protocolVersion !== 1) throw new Error('Invalid initialize response')
          initialized = true
          child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 2,
            method: '_iyw/side_question',
            params: { sessionId: 'validation-only', action: 'capabilities' } }) + '\n')
        } else if (value.id === 2) {
          const detail = value.error?.data?.details ?? value.error?.message
          if (detail !== 'Side-question session is not available') throw new Error('Missing side-question handler')
          registered = true
          child.stdin.end()
        }
      } catch (error) { failure = error; child.kill() }
    })
    child.stdin.on('error', (error) => { failure ??= error })
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize',
      params: { protocolVersion: 1, clientCapabilities: {},
        clientInfo: { name: 'iyw-native-bundle-verifier', version: '1.0.0' } } }) + '\n')
  })
}
