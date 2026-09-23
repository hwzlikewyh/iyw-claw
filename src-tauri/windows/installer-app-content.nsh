Function IywClawInspectBackupContent
  InitPluginsDir
  File /oname=$PLUGINSDIR\iyw-app-content.ps1 "${__FILEDIR__}\installer-app-content.ps1"
  nsExec::ExecToLog /TIMEOUT=10000 '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$PLUGINSDIR\iyw-app-content.ps1" -BackupDirectory "$IywClawBackupDir"'
  Pop $R0
  Delete "$PLUGINSDIR\iyw-app-content.ps1"
  Push $R0
FunctionEnd
