import { loadContext } from "./context.mjs"
import { authenticated, login, logout } from "./auth.mjs"
import { serve } from "./server.mjs"
import { stopChildren } from "./process.mjs"
import { prepare } from "./runtime.mjs"

process.once("SIGTERM", () => {
  stopChildren()
  process.exit(1)
})
process.once("SIGINT", () => {
  stopChildren()
  process.exit(1)
})

try {
  const context = await loadContext()
  const mode = process.argv[2]
  if (mode === "prepare") await prepare(context)
  else if (mode === "login") process.exitCode = await login(context)
  else if (mode === "logout") process.exitCode = await logout(context)
  else if (mode === "status") {
    const value = await authenticated(context)
    console.log(JSON.stringify({ authenticated: value }))
    process.exitCode = value ? 0 : 1
  } else if (mode === "serve") await serve(context)
  else
    throw new Error("用法：node adapter.mjs prepare|login|status|logout|serve")
} catch (error) {
  console.error(error.message)
  process.exitCode = 1
}
