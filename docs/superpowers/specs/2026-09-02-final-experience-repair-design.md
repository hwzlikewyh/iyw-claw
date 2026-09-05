# Final Experience Repair Design

## Scope

This repair covers three release-blocking regressions observed in the installed
Windows 0.1.161 client:

1. The assistant execution stream does not match the approved interaction.
2. Manual installation exposes uninstall-first behavior, can create `app\app`,
   and does not reliably create a desktop shortcut.
3. The managed browser cannot open when Fusion has no `browser-engine` offer.

The existing final-answer and task-artifact presentation remain unchanged.
Server packaging and unrelated runtime bootstrap work are out of scope.

## Conversation Execution Stream

Each assistant turn owns one execution surface. While the turn is live, all
reasoning, narration, plan, and ordinary tool-call parts render in source order
inside that surface instead of creating separate reasoning disclosures.

The live surface has a stable maximum height and scrolls internally. New content
follows the bottom while the user is already at the bottom. Scrolling upward
pauses following and reveals a compact "back to latest" control; clicking it or
returning to the bottom resumes following. Streaming must never pull a user away
from content they are inspecting.

New process rows enter with a short fade-and-rise transition. Running rows use a
restrained activity indicator; completion changes the status in place without
resizing the row. Expand and collapse animate height and opacity. Reduced-motion
users receive the same states without movement.

When the turn completes, the whole execution surface collapses after the final
state is committed. Its closed summary reports elapsed time, process count, and
error state. Historical completed turns start closed. Errors do not auto-open by
default, but remain visible in the closed summary. The final answer and existing
artifact area stay outside the process disclosure and remain visible.

## Windows Installer

The installer recognizes both the product-specific logical-root record and the
standard Tauri install record. A manual installer launched over a recognized old
installation relaunches in update mode and performs an in-place application
transaction instead of presenting uninstall-first as the default path.

Legacy installs whose executable directory already ends in `app` map that
directory to the new application directory and its parent to the logical root.
This prevents a second `app` segment. Existing `runtime`, `agents`, `skills`,
`inventory`, `config`, `data`, and logs stay under the logical root and are not
deleted during an update.

After every successful installation mode, the installer creates or retargets
the current user's `原助理.lnk` desktop shortcut to the installed
`iyw-claw.exe`. Uninstall removes only shortcuts that still target this product.

## Managed Browser

The bundled `agent-browser` remains byte-pinned and is not modified. Browser
startup requires a verified Chrome-for-Testing `browser-engine` managed
component. Fusion must publish a ready Windows x86_64 offer for the stable
channel before the repaired client is released.

The client preserves the upstream version-center error code when engine
resolution fails. `AGENT_TOOL_NOT_FOUND` is reported as a missing managed engine,
not collapsed into a generic runtime failure. Repeated clicks share one install
attempt and receive the same terminal result instead of issuing a burst of
identical resolve requests.

## Verification

- Component tests prove one ordered process surface, completed default collapse,
  follow/pause/resume scrolling, and reduced-motion behavior.
- Browser screenshots verify live, paused, completed, and reopened states without
  layout overlap at desktop and mobile widths.
- NSIS smoke tests cover legacy install migration, no `app\app`, update data
  preservation, and desktop shortcut target creation.
- Browser tests cover shared installation attempts and specific missing-offer
  error mapping. Installed-client acceptance proves the managed engine resolves,
  downloads, verifies, and opens a tab.
- A new release is published only after all three paths pass.
