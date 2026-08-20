!define IYW_CLAW_PROCESS_WAIT_ATTEMPTS 10
!define IYW_CLAW_PROCESS_WAIT_MS 500

Var IywClawAccountFilter
Var IywClawProcessError

!macro IywClawDefineProcessFunctions Prefix
Function ${Prefix}IywClawBuildAccountFilter
  ReadEnvStr $R0 "USERDOMAIN"
  ReadEnvStr $R1 "USERNAME"
  StrCmp $R1 "" account_filter_failed 0
  StrCmp $R0 "" account_filter_without_domain 0
  StrCpy $IywClawAccountFilter "$R0\$R1"
  Return

  account_filter_without_domain:
    StrCpy $IywClawAccountFilter "$R1"
    Return

  account_filter_failed:
    StrCpy $IywClawAccountFilter ""
FunctionEnd

Function ${Prefix}IywClawIssueKnownProcessKills
  nsExec::ExecToLog 'taskkill.exe /F /FI "USERNAME eq $IywClawAccountFilter" /IM "iyw-claw.exe"'
  Pop $R0
  nsExec::ExecToLog 'taskkill.exe /F /FI "USERNAME eq $IywClawAccountFilter" /IM "iyw-claw-mcp*.exe"'
  Pop $R0
  nsExec::ExecToLog 'taskkill.exe /F /T /FI "USERNAME eq $IywClawAccountFilter" /IM "agent-browser.exe"'
  Pop $R0
FunctionEnd

Function ${Prefix}IywClawCheckVersionedMcp
  ; tasklist 不支持 IMAGENAME 通配：先按当前用户列出，再匹配版本化文件名。
  nsExec::Exec 'cmd.exe /D /C tasklist.exe /FI "USERNAME eq $IywClawAccountFilter" /FO TABLE /NH > "$PLUGINSDIR\iyw-claw-mcp-processes.txt"'
  Pop $R0
  StrCmp $R0 "0" mcp_tasklist_ready mcp_check_failed

  mcp_tasklist_ready:
    nsExec::Exec 'cmd.exe /D /C findstr.exe /B /I /R /C:"iyw-claw-mcp-.*\.exe" "$PLUGINSDIR\iyw-claw-mcp-processes.txt" >NUL'
    Pop $R0
    Delete "$PLUGINSDIR\iyw-claw-mcp-processes.txt"
    StrCmp $R0 "0" mcp_process_found 0
    StrCmp $R0 "1" mcp_process_absent mcp_check_failed

  mcp_process_found:
    Push "1"
    Return

  mcp_process_absent:
    Push "0"
    Return

  mcp_check_failed:
    Delete "$PLUGINSDIR\iyw-claw-mcp-processes.txt"
    Push "2"
FunctionEnd

Function ${Prefix}IywClawFindCurrentUserProcess
  ; Avoid depending on nsis_tauri_utils during hook parsing. Tauri 2.11.x
  ; registers the additional plugin directory after installer hooks are
  ; included, so a hook-time plugin call breaks unsigned Windows builds.
  Pop $R8
  StrCmp $IywClawAccountFilter "" process_check_failed 0
  nsExec::Exec 'cmd.exe /D /C tasklist.exe /FI "USERNAME eq $IywClawAccountFilter" /FI "IMAGENAME eq $R8" /FO TABLE /NH > "$PLUGINSDIR\iyw-claw-process-check.txt"'
  Pop $R0
  StrCmp $R0 "0" process_tasklist_ready process_check_failed

  process_tasklist_ready:
    nsExec::Exec 'cmd.exe /D /C findstr.exe /B /I /C:"$R8" "$PLUGINSDIR\iyw-claw-process-check.txt" >NUL'
    Pop $R0
    Delete "$PLUGINSDIR\iyw-claw-process-check.txt"
    StrCmp $R0 "0" process_found 0
    StrCmp $R0 "1" process_absent process_check_failed

  process_found:
    Push "0"
    Return

  process_absent:
    Push "1"
    Return

  process_check_failed:
    Delete "$PLUGINSDIR\iyw-claw-process-check.txt"
    Push "2"
FunctionEnd

Function ${Prefix}IywClawAnyKnownProcessRunning
  Push "iyw-claw.exe"
  Call ${Prefix}IywClawFindCurrentUserProcess
  Pop $R0
  StrCmp $R0 "0" main_process_running 0
  StrCmp $R0 "1" check_browser process_check_failed

  check_browser:
    Push "agent-browser.exe"
    Call ${Prefix}IywClawFindCurrentUserProcess
    Pop $R0
    StrCmp $R0 "0" browser_process_running 0
    StrCmp $R0 "1" check_generic_mcp process_check_failed

  check_generic_mcp:
    Push "iyw-claw-mcp.exe"
    Call ${Prefix}IywClawFindCurrentUserProcess
    Pop $R0
    StrCmp $R0 "0" mcp_process_running 0
    StrCmp $R0 "1" check_versioned_mcp process_check_failed

  check_versioned_mcp:
    Call ${Prefix}IywClawCheckVersionedMcp
    Pop $R0
    StrCmp $R0 "0" no_known_process 0
    StrCmp $R0 "1" mcp_process_running process_check_failed

  main_process_running:
    StrCpy $IywClawProcessError "iyw-claw.exe"
    Goto known_process_running
  browser_process_running:
    StrCpy $IywClawProcessError "agent-browser.exe"
    Goto known_process_running
  mcp_process_running:
    StrCpy $IywClawProcessError "iyw-claw-mcp*.exe"
  known_process_running:
    Push "1"
    Return

  process_check_failed:
    StrCpy $IywClawProcessError "进程状态检查失败"
    Push "2"
    Return

  no_known_process:
    Push "0"
FunctionEnd

Function ${Prefix}IywClawStopKnownProcesses
  StrCpy $IywClawProcessError ""
  Call ${Prefix}IywClawBuildAccountFilter
  StrCmp $IywClawAccountFilter "" stop_processes_failed 0
  DetailPrint "正在停止当前用户的 iyw-claw 后台进程..."
  Call ${Prefix}IywClawIssueKnownProcessKills
  StrCpy $R4 0

  wait_for_processes:
    Sleep ${IYW_CLAW_PROCESS_WAIT_MS}
    Call ${Prefix}IywClawAnyKnownProcessRunning
    Pop $R5
    StrCmp $R5 "0" stop_processes_done 0
    StrCmp $R5 "2" stop_processes_failed 0
    IntOp $R4 $R4 + 1
    IntCmp $R4 ${IYW_CLAW_PROCESS_WAIT_ATTEMPTS} stop_processes_timeout retry_process_kill stop_processes_timeout

  retry_process_kill:
    Call ${Prefix}IywClawIssueKnownProcessKills
    Goto wait_for_processes

  stop_processes_timeout:
    DetailPrint "等待进程退出超时：$IywClawProcessError"
    Push "1"
    Return

  stop_processes_failed:
    StrCmp $IywClawProcessError "" 0 +2
      StrCpy $IywClawProcessError "无法确定当前用户"
    DetailPrint "无法安全停止进程：$IywClawProcessError"
    Push "1"
    Return

  stop_processes_done:
    DetailPrint "当前用户的 iyw-claw 后台进程已停止。"
    Push "0"
FunctionEnd
!macroend

!insertmacro IywClawDefineProcessFunctions ""
!insertmacro IywClawDefineProcessFunctions "un."

Function IywClawIsMainProcessRunning
  ; 仅检查安装器当前用户，其他登录会话不能改变本次恢复策略。
  Call IywClawBuildAccountFilter
  Push "iyw-claw.exe"
  Call IywClawFindCurrentUserProcess
  Pop $R5
  StrCmp $R5 "0" iyw_process_running 0
  Push "0"
  Return

  iyw_process_running:
    Push "1"
FunctionEnd
