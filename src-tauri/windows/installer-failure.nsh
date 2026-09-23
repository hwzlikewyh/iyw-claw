Var IywClawFailureHandled
Var IywClawFailureHadTransaction
Var IywClawFailureReason
Var IywClawRecoveryStatus

Function IywClawHandleInstallFailure
  StrCmp $IywClawFailureHandled "1" iyw_failure_done 0
  StrCpy $IywClawFailureHandled "1"
  StrCpy $IywClawFailureReason "$IywClawTransactionError"
  StrCpy $IywClawFailureHadTransaction "$IywClawTransactionActive"
  StrCmp $IywClawElevationRolledBack "1" 0 +2
  StrCpy $IywClawFailureHadTransaction "1"
  StrCmp $IywClawFailureReason "" 0 iyw_failure_reason_ready
  StrCpy $IywClawFailureReason "安装文件写入失败或安装被中止，请查看安装详情。"
  iyw_failure_reason_ready:
  Call IywClawRollbackAppTransaction
  Pop $R0
  StrCmp $IywClawFailureHadTransaction "1" 0 iyw_failure_no_transaction
  StrCmp $R0 "1" 0 iyw_failure_recovery_failed
  StrCmp $IywClawTransactionHasBackup "1" 0 iyw_failure_no_previous
  StrCpy $IywClawRecoveryStatus "旧版本已恢复。"
  Goto iyw_failure_report
  iyw_failure_no_previous:
    StrCpy $IywClawRecoveryStatus "首次安装未完成，没有旧版本可恢复。修复问题后可重新运行安装包。"
    Goto iyw_failure_report
  iyw_failure_recovery_failed:
    StrCpy $IywClawRecoveryStatus "旧版本恢复失败，备份已保留：$IywClawBackupDir"
    Goto iyw_failure_report
  iyw_failure_no_transaction:
    StrCpy $IywClawRecoveryStatus "应用替换事务未开始或已提交，本次未执行回滚。"
  iyw_failure_report:
    DetailPrint "$IywClawFailureReason"
    DetailPrint "$IywClawRecoveryStatus"
    Push "failure: $IywClawFailureReason; recovery: $IywClawRecoveryStatus"
    Call IywClawAppendInstallerLog
    Call IywClawRestartOldAppIfRequested
    IfSilent iyw_failure_done 0
    StrCmp $PassiveMode "1" iyw_failure_done 0
    MessageBox MB_OK|MB_ICONSTOP "$IywClawFailureReason$\r$\n$\r$\n$IywClawRecoveryStatus$\r$\n$\r$\n日志：$IywClawRoot\logs\installer.log"
  iyw_failure_done:
FunctionEnd
