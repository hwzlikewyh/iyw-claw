const BOOTSTRAP_TIMEOUT_MS = 10_000

// 上游Web入口会强制写入深色主题；就绪前恢复已保存的界面设置，再开启持久写入。
export async function restoreInterface(wasm, saved) {
  if (!saved) return
  const settings = JSON.parse(saved).interface
  if (!settings) return
  const id = crypto.randomUUID()
  wasm.craft_submit(
    JSON.stringify({
      id,
      name: "command_run",
      arguments: {
        id: "prefs.set",
        params: { path: "interface", value: settings },
      },
    })
  )
  const deadline = Date.now() + BOOTSTRAP_TIMEOUT_MS
  while (Date.now() < deadline) {
    const reply = JSON.parse(wasm.craft_take_reply())
    if (reply?.id === id) {
      if (reply.result.isError)
        throw Error("Saved editor interface settings could not be restored")
      await new Promise((resolve) => requestAnimationFrame(resolve))
      return
    }
    await new Promise((resolve) => setTimeout(resolve, 16))
  }
  throw Error(
    "Restoring editor settings timed out; saved settings were retained"
  )
}
