; Tauri NSIS installer hooks.
;
; iyw-claw-mcp.exe is the MCP stdio companion spawned by each agent CLI
; (claude / codex / opencode / ...), which is itself a grandchild of
; iyw-claw.exe. Windows does not propagate parent death to descendants the
; way Unix does, so stale iyw-claw-mcp.exe processes from a previous session
; can keep the binary file locked. The installer then fails to overwrite
; it with:
;
;     Error opening file for writing: ...\iyw-claw\iyw-claw-mcp.exe
;
; Stop any running companion processes before the installer writes new
; binaries (or removes the existing ones on uninstall). taskkill returns
; non-zero when no processes match, which is fine — we ignore the result.

; Capture this path while NSIS is including this source file. Inside hook
; macros, `${__FILEDIR__}` is evaluated again at the generated installer call
; site and would incorrectly point into `target/release/nsis/x64`.
!define IYW_CLAW_INSTALL_MANAGED_NODE_SCRIPT "${__FILEDIR__}\..\..\install-managed-node.ps1"

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping any running iyw-claw-mcp processes..."
  nsExec::Exec 'taskkill /F /T /IM iyw-claw-mcp.exe'
  Pop $0
  ; Small grace period so the OS releases file handles before the
  ; installer attempts to overwrite iyw-claw-mcp.exe.
  Sleep 500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Preparing private Node.js/npm runtime..."

  ; $INSTDIR is the selected app directory. Keep runtime beside it so an
  ; application update only replaces files below $INSTDIR.
  ; The helper lives at the project root and is embedded into the installer
  ; temporary directory only while this section is running.
  File /oname=$PLUGINSDIR\install-managed-node.ps1 "${IYW_CLAW_INSTALL_MANAGED_NODE_SCRIPT}"

  nsExec::ExecToLog 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\install-managed-node.ps1" -Version "24.0.0" -RuntimeRoot "$INSTDIR\..\runtime"'
  Pop $0

  ${If} $0 != 0
    MessageBox MB_ABORTRETRYIGNORE|MB_ICONEXCLAMATION \
      "Node.js/npm runtime installation failed with exit code $0.$\r$\n$\r$\nRetry the installation, ignore it and let iyw-claw download the runtime later, or abort setup." \
      IDRETRY retry_node_runtime IDIGNORE node_runtime_done
    Abort

    retry_node_runtime:
      nsExec::ExecToLog 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\install-managed-node.ps1" -Version "24.0.0" -RuntimeRoot "$INSTDIR\..\runtime"'
      Pop $0
      ${If} $0 != 0
        MessageBox MB_OK|MB_ICONEXCLAMATION \
          "Node.js/npm runtime installation failed again. iyw-claw was installed, but npm-based Agents will require the runtime to be installed later."
      ${EndIf}
  ${EndIf}

  node_runtime_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Stopping any running iyw-claw-mcp processes..."
  nsExec::Exec 'taskkill /F /T /IM iyw-claw-mcp.exe'
  Pop $0
  Sleep 500
!macroend
