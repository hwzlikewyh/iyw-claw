import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"

function installerSource(name: string): string {
  return readFileSync(resolve("src-tauri", "windows", name), "utf8")
}

describe("Windows installer compatibility contract", () => {
  it("recovers a legacy app directory before resolving the logical root", () => {
    const hooks = installerSource("installer-hooks.nsh")
    const migration = installerSource("installer-install-root.nsh")

    expect(
      hooks.indexOf("Call IywClawNormalizeLegacyInstallRoot")
    ).toBeLessThan(
      hooks.indexOf('ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey"')
    )
    expect(migration).toContain(
      'ReadRegStr $R8 SHCTX "${UNINSTKEY}" "InstallLocation"'
    )
    expect(migration).toContain(
      'StrCmp $IywClawInstallerTestMode "1" iyw_legacy_root_done 0'
    )
    expect(migration).not.toContain(
      'ReadRegStr $R8 SHCTX "${MANUPRODUCTKEY}" "InstallLocation"'
    )
    expect(migration).toContain('StrCpy $R8 $R8 "" 1')
    expect(migration).toContain('IfFileExists "$R8\\iyw-claw.exe"')
    expect(migration).toContain('GetFullPathName $R7 "$R9\\app"')
    expect(migration).toContain(
      'WriteRegStr SHCTX "$IywClawInstallRegistryKey" "InstallRoot" "$R9"'
    )
  })

  it("creates a desktop shortcut after the app transaction commits", () => {
    const hooks = installerSource("installer-hooks.nsh")
    const shortcut = installerSource("installer-desktop-shortcut.nsh")

    expect(hooks.indexOf("Call IywClawCommitAppTransaction")).toBeLessThan(
      hooks.indexOf("Call IywClawEnsureDesktopShortcut")
    )
    expect(shortcut).toContain(
      'CreateShortcut "$DESKTOP\\${PRODUCTNAME}.lnk" "$IywClawAppDir\\${MAINBINARYNAME}.exe"'
    )
    expect(shortcut).toContain("SetLnkAppUserModelId")
  })
})
