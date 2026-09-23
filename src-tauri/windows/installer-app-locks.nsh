Function IywClawReportAppLocks
  InitPluginsDir
  File /oname=$PLUGINSDIR\iyw-app-locks.ps1 "${__FILEDIR__}\installer-app-locks.ps1"
  nsExec::ExecToStack /TIMEOUT=10000 '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$PLUGINSDIR\iyw-app-locks.ps1" -AppDirectory "$IywClawAppDir" -ReleaseExplorer'
  Pop $R0
  Pop $R1
  Delete "$PLUGINSDIR\iyw-app-locks.ps1"
  DetailPrint "app 占用诊断（exit=$R0）：$R1"
  Push "app lock inspection (exit=$R0): $R1"
  Call IywClawAppendInstallerLog
FunctionEnd
