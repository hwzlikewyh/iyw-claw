---
name: infinite-canvas-open
description: Open the installed Infinite Canvas MCP App for the current workspace.
---

Use the iyw-claw capability gateway and invoke `plugin.infinite-canvas.canvas.render-canvas.v1`.
Pass the requested display mode and a validated canvas ID. If the capability reports that the
plugin is not installed or authorized, ask the host to install or authorize it; never download a
runtime, start a local web server, or search for a native MCP namespace.

After opening, use `plugin.infinite-canvas.canvas.get-canvas-state.v1` to confirm the canvas ID and
revision. Do not discover or migrate legacy Cowart pages unless the user explicitly asks for
migration; when asked, call the migration capability with `listOnly: true` first and present the
read-only page list before previewing or confirming a target.
