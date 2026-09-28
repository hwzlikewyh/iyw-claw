Var IywClawPermissionAction
Var IywClawPermissionResult
Var IywClawOriginalSid
Var IywClawElevationChecked
Var IywClawSelectedRoot

Function IywClawRunPermissionCheck
  InitPluginsDir
  File /oname=$PLUGINSDIR\iyw-permissions.ps1 "${__FILEDIR__}\installer-permissions.ps1"
  Delete "$PLUGINSDIR\iyw-permissions.ini"
  ; 通过栈传递路径，避免路径内的引号被 System 插件再次解析。
  Push $IywClawRoot
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_ROOT", w s)'
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_EXPECTED_SID", w "$IywClawOriginalSid")'
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$PLUGINSDIR\iyw-permissions.ps1" -Action "$IywClawPermissionAction"'
  Pop $IywClawPermissionResult
  StrCpy $IywClawPermissionError "权限检查程序执行失败（退出码=$IywClawPermissionResult）。"
  ReadINIStr $IywClawElevated "$PLUGINSDIR\iyw-permissions.ini" "result" "Elevated"
  ReadINIStr $R0 "$PLUGINSDIR\iyw-permissions.ini" "result" "Root"
  StrCmp $R0 "" +2 0
  StrCpy $IywClawRoot $R0
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
  StrCpy $IywClawPermissionError "$IywClawPermissionError 当前用户无法写入目录，请选择有写入权限的安装目录（默认位于当前用户的 LocalAppData），并检查用户环境目录的权限。"
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
