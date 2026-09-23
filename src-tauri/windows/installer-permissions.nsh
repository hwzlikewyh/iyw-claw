Var IywClawPermissionAction
Var IywClawPermissionResult
Var IywClawOriginalSid
Var IywClawPermissionLaunched
Var IywClawElevationChecked
Var IywClawSelectedRoot

Function IywClawRunPermissionCheck
  InitPluginsDir
  File /oname=$PLUGINSDIR\iyw-permissions.ps1 "${__FILEDIR__}\installer-permissions.ps1"
  Delete "$PLUGINSDIR\iyw-permissions.ini"
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_ROOT", w "$IywClawRoot")'
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_EXPECTED_SID", w "$IywClawOriginalSid")'
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$PLUGINSDIR\iyw-permissions.ps1" -Action "$IywClawPermissionAction"'
  Pop $IywClawPermissionResult
  StrCpy $IywClawPermissionError "权限检查程序执行失败（退出码=$IywClawPermissionResult）。"
  ReadINIStr $IywClawElevated "$PLUGINSDIR\iyw-permissions.ini" "result" "Elevated"
  ReadINIStr $IywClawPermissionLaunched "$PLUGINSDIR\iyw-permissions.ini" "result" "Launched"
  ReadINIStr $R0 "$PLUGINSDIR\iyw-permissions.ini" "result" "Error"
  StrCmp $R0 "" +2 0
  StrCpy $IywClawPermissionError $R0
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_ROOT", p 0)'
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_EXPECTED_SID", p 0)'
FunctionEnd

Function IywClawValidateElevationIdentity
  StrCmp $IywClawElevationChecked "1" identity_done 0
  StrCpy $IywClawElevationChecked "1"
  StrCmp $IywClawInstallerTestMode "1" identity_done 0
  ClearErrors
  ${GetOptions} $CMDLINE "/IYW_ORIGINAL_SID=" $IywClawOriginalSid
  IfErrors identity_done 0
  StrCpy $IywClawPermissionAction "identity"
  Call IywClawRunPermissionCheck
  StrCmp $IywClawPermissionResult "0" identity_validated 0
  DetailPrint "$IywClawPermissionError"
  IfSilent identity_quit 0
  MessageBox MB_OK|MB_ICONSTOP "无法以原用户身份继续安装。请从原登录账户重新运行安装包。$\r$\n$IywClawPermissionError"
  identity_quit:
  SetErrorLevel 1
  Quit
  identity_validated:
  ${GetOptions} $CMDLINE "/IYW_SELECTED_ROOT=" $R0
  IfErrors identity_quit 0
  GetFullPathName $IywClawSelectedRoot "$R0"
  StrCpy $INSTDIR "$IywClawSelectedRoot"
  identity_done:
FunctionEnd

Function IywClawMeasureInstallSize
  StrCpy $R0 0
  StrCpy $R1 0
  measure_next_section:
  ClearErrors
  SectionGetSize $R0 $R2
  IfErrors measure_sections_done 0
  IntOp $R1 $R1 + $R2
  IntOp $R0 $R0 + 1
  Goto measure_next_section
  measure_sections_done:
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_REQUIRED_KB", w "$R1")'
FunctionEnd

Function IywClawCheckInstallPermissions
  StrCmp $IywClawInstallerTestMode "1" permissions_ready 0
  Call IywClawMeasureInstallSize
  StrCpy $IywClawPermissionAction "check"
  Call IywClawRunPermissionCheck
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_REQUIRED_KB", p 0)'
  StrCmp $IywClawPermissionResult "0" permissions_ready 0
  StrCmp $IywClawPermissionResult "31" 0 permissions_failed
  Call IywClawTryElevatedInstall
  permissions_failed:
  StrCpy $IywClawTransactionError "$IywClawPermissionError"
  DetailPrint "$IywClawPermissionError"
  IfSilent permissions_abort 0
  MessageBox MB_OK|MB_ICONSTOP "安装前检查未通过：$\r$\n$IywClawPermissionError$\r$\n请检查目录权限、安全软件和磁盘空间。"
  permissions_abort:
  SetErrorLevel 1
  Abort
  permissions_ready:
FunctionEnd

Function IywClawTryElevatedInstall
  StrCmp $IywClawInstallerTestMode "1" elevation_unavailable 0
  StrCmp $IywClawElevated "0" 0 elevation_unavailable
  StrCmp $IywClawOriginalSid "" 0 elevation_unavailable
  IfSilent elevation_silent 0
  ; 必须先回滚；提权后的安装器会重新走权限检查及正常安装事务。
  StrCpy $IywClawElevationRolledBack $IywClawTransactionActive
  Call IywClawRollbackAppTransaction
  Pop $R0
  StrCmp $R0 "1" 0 elevation_rollback_failed
  Call IywClawLaunchElevatedInstaller
  Return
  elevation_silent:
  StrCpy $IywClawPermissionError "$IywClawPermissionError 静默安装不会请求 UAC，请由原用户以管理员权限启动安装器。"
  Return
  elevation_rollback_failed:
  StrCpy $IywClawElevationRolledBack "failed"
  StrCpy $IywClawPermissionError "提权前恢复旧版本失败，已停止重试并保留备份。"
  elevation_unavailable:
FunctionEnd

Function IywClawLaunchElevatedInstaller
  ${GetParameters} $R0
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_ARGUMENTS", w "$R0")'
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_EXECUTABLE", w "$EXEPATH")'
  SetOutPath "$TEMP"
  DetailPrint "目录访问被拒绝，正在申请原用户的管理员权限..."
  StrCpy $IywClawPermissionAction "elevate"
  Call IywClawRunPermissionCheck
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_ARGUMENTS", p 0)'
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_EXECUTABLE", p 0)'
  StrCmp $IywClawPermissionLaunched "1" elevation_finished 0
  StrCmp $IywClawPermissionResult "1223" 0 elevation_launch_done
  StrCpy $IywClawPermissionError "已取消管理员权限授权，安装未继续。"
  Return
  elevation_finished:
  StrCmp $IywClawPermissionResult "0" +2 0
  Call IywClawRestartOldAppIfRequested
  StrCpy $IywClawFailureHandled "1"
  SetErrorLevel $IywClawPermissionResult
  Quit
  elevation_launch_done:
FunctionEnd
