"use client"

import { isDesktop } from "@/lib/platform"
import { usePlatform } from "@/hooks/use-platform"
import { WindowControls } from "./window-controls"

/**
 * Window controls pinned to the top-right of the viewport, above a modal dialog.
 *
 * The startup gates (login, Codex bootstrap) cover the app with a blocking
 * dialog *before* the workspace — and therefore `AppTitleBar`, the usual home of
 * `WindowControls` — has mounted. Without this the window cannot be minimized or
 * closed at all while a gate is up, which strands the user on a bootstrap that
 * is slow or has failed. Mount it alongside a gate dialog and pass
 * `visible={blocked}`.
 *
 * Two details make it work on top of a Radix modal:
 *  - `z-[110]` clears the dialog overlay and content (both `z-50`) and the Linux
 *    resize grips (`z-[100]`), whose top edge already carves out the controls
 *    region so the two never fight over the corner.
 *  - `pointer-events-auto` is required: a modal dialog sets `pointer-events:
 *    none` on `<body>` while open, which this would otherwise inherit. The
 *    wrapper is `pointer-events-none` so only the buttons themselves take
 *    clicks, leaving the rest of the strip inert.
 *
 * The gates suppress `onPointerDownOutside`/`onInteractOutside`, so clicking
 * these buttons cannot dismiss the dialog. Radix's focus trap does pull focus
 * back to the dialog on click, but the click itself lands first, so the button
 * fires. Radix also marks everything outside the dialog `aria-hidden` while it
 * is open, so these are not reachable by screen reader during a gate; the OS
 * keyboard equivalents (Alt+F4, Win+Down) are unaffected.
 */
export function OverlayWindowControls({ visible }: { visible: boolean }) {
  const { isWindows, isLinux } = usePlatform()

  // Same guard as WindowControls: macOS keeps its native traffic lights, and the
  // web build has no window to control. Checked here too so the wrapper element
  // is not emitted at all on those platforms.
  if (!visible || !(isWindows || isLinux) || !isDesktop()) return null

  return (
    <div className="pointer-events-none fixed top-0 right-0 z-[110] [&_button]:pointer-events-auto">
      <WindowControls tone="overlay" />
    </div>
  )
}
