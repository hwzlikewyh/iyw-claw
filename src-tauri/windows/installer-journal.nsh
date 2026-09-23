Var IywClawPendingFile
Var IywClawPartialDir

Function IywClawWritePendingTransaction
  StrCpy $IywClawPendingFile "$IywClawRoot\staging\installer-app-pending-v1"
  ClearErrors
  FileOpen $R0 "$IywClawPendingFile" w
  IfErrors pending_write_failed 0
  FileWrite $R0 "pending$\r$\n"
  IfErrors pending_close_failed 0
  System::Call 'kernel32::FlushFileBuffers(p R0) i.R1'
  FileClose $R0
  StrCmp $R1 "0" pending_write_failed 0
  Push "1"
  Return
  pending_close_failed:
    FileClose $R0
  pending_write_failed:
    StrCpy $IywClawTransactionError "无法记录安装事务，旧版本尚未替换"
    Push "0"
FunctionEnd

Function IywClawClearPendingTransaction
  ClearErrors
  Delete "$IywClawPendingFile"
  IfFileExists "$IywClawPendingFile" pending_clear_failed 0
  Push "1"
  Return
  pending_clear_failed:
    StrCpy $IywClawTransactionError "无法完成安装事务记录，旧版本备份仍保留"
    Push "0"
FunctionEnd

Function IywClawPreservePartialApp
  IfFileExists "$IywClawAppDir" 0 partial_app_absent
  ClearErrors
  GetTempFileName $IywClawPartialDir "$IywClawRoot\staging"
  IfErrors partial_app_failed 0
  Delete "$IywClawPartialDir"
  IfErrors partial_app_failed 0
  StrCpy $IywClawPartialDir "$IywClawPartialDir-incomplete-app"
  Rename "$IywClawAppDir" "$IywClawPartialDir"
  IfErrors partial_app_failed 0
  Push "retained incomplete app: $IywClawPartialDir"
  Call IywClawAppendInstallerLog
  partial_app_absent:
    Push "1"
    Return
  partial_app_failed:
    StrCpy $IywClawTransactionError "无法保留未完成的应用目录，已停止恢复并保留现场"
    Push "0"
FunctionEnd

Function IywClawReconcilePendingTransaction
  StrCpy $IywClawPendingFile "$IywClawRoot\staging\installer-app-pending-v1"
  IfFileExists "$IywClawPendingFile" 0 pending_recovery_ready
  IfFileExists "$IywClawBackupDir" 0 pending_recovery_clear
  Push "$IywClawBackupDir"
  Call IywClawIsAppComplete
  Pop $R0
  StrCmp $R0 "1" 0 pending_recovery_failed
  Call IywClawPreservePartialApp
  Pop $R0
  StrCmp $R0 "1" 0 pending_recovery_failed
  ClearErrors
  Rename "$IywClawBackupDir" "$IywClawAppDir"
  IfErrors pending_recovery_failed 0
  Push "recovered uncommitted app transaction"
  Call IywClawAppendInstallerLog
  pending_recovery_clear:
    Call IywClawClearPendingTransaction
    Return
  pending_recovery_ready:
    Push "1"
    Return
  pending_recovery_failed:
    StrCpy $IywClawTransactionError "上次安装未提交且无法自动恢复，已保留 app、备份和事务记录"
    Push "0"
FunctionEnd
