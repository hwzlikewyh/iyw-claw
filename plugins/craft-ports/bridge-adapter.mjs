export function adaptEditorBridge(source, replaceOne) {
  source = replaceOne(
    source,
    ", this.timeout);\n      const item = { request, finish, dispatched: false };",
    ", prepared.iywUserAction ? 31e4 : this.timeout);\n      const item = { request, finish, dispatched: false };"
  )
  source = replaceOne(
    source,
    '  internal("_editor_attach", {}, async () =>',
    '  server2.registerTool("_iyw_user_menu", { inputSchema: { id: external_exports.string().min(1).max(256) }, _meta: hidden }, async ({ id }, extra) => bridge.call("ui_menu_invoke", { id }, extra.signal, async () => ({ iywUserAction: true })));\n  internal("_editor_attach", {}, async () =>'
  )
  source = replaceOne(
    source,
    "      this.check(token);\n      const item = [...this.requests.values()].find((value) => !value.dispatched);",
    '      if (token !== this.token) throw new Error("Editor connection expired; reopen the panel");\n      const item = [...this.requests.values()].find((value) => !value.dispatched);'
  )
  source = replaceOne(
    source,
    "    } finally {\n      this.polling = false;\n      this.wake = null;\n    }\n  }\n  reply(token, id, result)",
    "    } finally {\n      if (this.token === token) { this.polling = false; this.wake = null; }\n    }\n  }\n  reply(token, id, result)"
  )
  return replaceOne(
    source,
    "    for (const item of this.requests.values()) item.finish(failure2(reason));\n    this.wake?.();",
    "    for (const item of this.requests.values()) item.finish(failure2(reason));\n    this.wake?.();\n    this.polling = false;\n    this.wake = null;"
  )
}
