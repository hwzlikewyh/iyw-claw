import { AsyncLocalStorage } from "node:async_hooks"

const MAX_EDITOR_CONVERSATIONS = 8

export function installConversationScope(server) {
  const current = new AsyncLocalStorage(),
    conversations = new Map()
  const register = server.registerTool.bind(server)
  server.registerTool = (name, config, callback) =>
    register(name, config, (args, extra) => {
      const id = extra?._meta?.["iyw/conversationId"]
      if (typeof id !== "string" || !/^[1-9][0-9]{0,9}$/.test(id))
        throw Error(
          "Trusted original-assistant conversation context is required"
        )
      return current.run(id, async () => {
        const result = await callback(args, extra)
        if (name === "_editor_detach" && !result.isError)
          conversations.delete(id)
        return result
      })
    })
  const proxy = (key, create) =>
    new Proxy(
      {},
      {
        get(_target, property) {
          if (property === "then") return undefined
          const id = current.getStore()
          if (!id) throw Error("Editor conversation context is unavailable")
          if (!conversations.has(id)) {
            if (conversations.size >= MAX_EDITOR_CONVERSATIONS)
              throw Error(
                "Too many editor conversations; close unused editor pages"
              )
            conversations.set(id, {})
          }
          const state = conversations.get(id)
          const object = (state[key] ??= create())
          const value = Reflect.get(object, property, object)
          return typeof value === "function" ? value.bind(object) : value
        },
      }
    )
  return { proxy, id: () => current.getStore() }
}
