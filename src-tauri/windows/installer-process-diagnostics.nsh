; 安装和卸载复用执行入口；持久日志仅由安装端写入。
!macro IywClawDefineProcessDiagnostics Prefix
Function ${Prefix}IywClawRunKnownProcessCommandAt
  Pop $R8
  Pop $R9
  Call ${Prefix}IywClawWriteKnownProcessScript
  Pop $R0
  StrCmp $R0 "0" known_process_script_ready known_process_command_failed

  known_process_script_ready:
  System::Call 'kernel32::GetCurrentProcessId() i.R7'
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\iyw-claw-process-control.ps1" -Action "$R8" -InstallDir "$R9" -InstallerProcessId $R7'
  Pop $R0
  Pop $R1
  StrCmp $R0 "0" known_process_command_cleanup
  StrCmp $R0 "1" known_process_command_cleanup
  StrCpy $IywClawProcessError "action=$R8; exit=$R0; output=$R1"
  !if "${Prefix}" == ""
  StrCmp $IywClawRoot "" known_process_command_cleanup
  Push "process command failed: action=$R8; exit=$R0; install_dir=$R9; output=$R1"
  Call IywClawAppendInstallerLog
  CreateDirectory "$IywClawRoot\logs"
  CopyFiles /SILENT "$PLUGINSDIR\iyw-claw-process-control.log" "$IywClawRoot\logs\installer-process-control.log"
  !endif
  known_process_command_cleanup:
  Delete "$PLUGINSDIR\iyw-claw-process-control.ps1"
  Delete "$PLUGINSDIR\iyw-restart-manager.cs"
  Delete "$PLUGINSDIR\iyw-claw-process-control.log"
  StrCmp $R0 "0" known_process_command_result_ready 0
  StrCmp $R0 "1" known_process_command_result_ready 0
  StrCmp $R0 "2" known_process_command_result_ready 0
  StrCpy $R0 "2"

  known_process_command_result_ready:
  Push $R0
  Return

  known_process_command_failed:
    StrCpy $IywClawProcessError "无法写入进程检查脚本：$PLUGINSDIR\iyw-claw-process-control.ps1"
    Delete "$PLUGINSDIR\iyw-claw-process-control.ps1"
    Delete "$PLUGINSDIR\iyw-restart-manager.cs"
    Push "2"
FunctionEnd
!macroend
