---
name: iyw-capability-gateway
description: Use only when the current tool list exposes search_iyw_capabilities, read_iyw_capability, and invoke_iyw_capability together, either as bare names or under one shared host namespace, and an iyw-claw hosted capability may help. Otherwise use only visible direct tools; never assume the gateway exists.
---

# IYW Capability Gateway

## Activation gate

Use this gateway workflow only when the current tool list exposes all three
gateway capabilities together. They may appear as bare names:

- `search_iyw_capabilities`
- `read_iyw_capability`
- `invoke_iyw_capability`

Some hosts add one namespace to every MCP tool, for example
`mcp__iyw-claw-builtin-<session>__search_iyw_capabilities`. The bounded session
suffix is launch-specific. A namespaced gateway is valid only when all three visible
names use the same prefix and end in the three exact suffixes above. Do not
combine matching suffixes from different namespaces, and do not invent a prefix
that is absent from the current tool list.

If any one is missing, do not invoke, infer, or reconstruct the gateway. In a
legacy route, OpenClaw, Pi, or any session without the gateway, use only the
direct tools that are currently visible and follow their current schemas.

## Gateway workflow

When the activation gate passes, use the tools in this order:

1. Call `search_iyw_capabilities` with concise English goal keywords. When the
   user's goal is in another language, translate the intent into English for
   the search query; do not pass untranslated text and assume the catalog has
   multilingual aliases.
2. Choose a returned stable `capability_id` and call
   `read_iyw_capability` to inspect its full description and input schema.
3. Call `invoke_iyw_capability` with that exact id and an `arguments` object
   that conforms to the schema.

When the host exposes namespaced tool names, call the corresponding visible
namespaced tool. The bare names in this guide identify the gateway roles; they
do not authorize constructing a tool name that the host did not expose.

Search again when no result fits. Read again when the selected capability or
required arguments are uncertain. Never derive an id from a remembered tool
name, and never pass a raw tool name to the invocation gateway.

## Delivery acknowledgement

When an `invoke_iyw_capability` response contains `iyw_delivery_receipt`, keep
that exact value. If there is a later capability invocation, return it in the
next `invoke_iyw_capability` request as top-level `delivery_ack`:

```json
{
  "capability_id": "<stable id>",
  "arguments": {},
  "delivery_ack": "<exact iyw_delivery_receipt>"
}
```

`delivery_ack` is a sibling of `capability_id` and `arguments`; never put it or
the receipt inside business `arguments`. If no later invocation occurs, leave
the receipt unacknowledged. Do not invent an acknowledgement or fabricate an
invocation only to send one.

## Session boundary

The catalog is scoped to the current iyw-claw session. Missing capabilities
may be disabled for this session; do not attempt to bypass that boundary by
editing Agent MCP configuration or writing bearer tokens to global files.

For destructive or externally visible actions, preserve the user-confirmation
rules in the selected capability description and the current conversation.
