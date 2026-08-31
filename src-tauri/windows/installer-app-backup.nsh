!define IYW_CLAW_APP_RENAME_ATTEMPTS 5
!define IYW_CLAW_APP_RENAME_WAIT_MS 500
!define IYW_CLAW_BACKUP_REMOVE_ATTEMPTS 5
!define IYW_CLAW_BACKUP_REMOVE_WAIT_MS 500
!define IYW_CLAW_RECOVERY_TARGET_ATTEMPTS 10
!define IYW_CLAW_RECOVERY_RENAME_ATTEMPTS 3

Function IywClawBackupCurrentAppWithRetry
  StrCpy $R2 1
  backup_current_app_retry:
    ClearErrors
    Rename "$IywClawAppDir" "$IywClawBackupDir"
    IfErrors backup_current_app_retry_failed 0
    Push "1"
    Return

  backup_current_app_retry_failed:
    IfFileExists "$IywClawBackupDir" backup_current_app_target_exists 0
    IntCmp $R2 ${IYW_CLAW_APP_RENAME_ATTEMPTS} backup_current_app_failed backup_current_app_wait backup_current_app_failed
  backup_current_app_wait:
    DetailPrint "旧 app 原子备份失败，等待后重试（$R2/${IYW_CLAW_APP_RENAME_ATTEMPTS}）：$IywClawAppDir -> $IywClawBackupDir"
    IntOp $R2 $R2 + 1
    Sleep ${IYW_CLAW_APP_RENAME_WAIT_MS}
    Goto backup_current_app_retry

  backup_current_app_target_exists:
    DetailPrint "旧 app 原子备份失败：source=$IywClawAppDir; target=$IywClawBackupDir; target_exists=1; attempts=$R2"
    StrCpy $IywClawTransactionError "无法原子备份旧 app：backup 目标已存在"
    Push "0"
    Return
  backup_current_app_failed:
    DetailPrint "旧 app 原子备份失败：source=$IywClawAppDir; target=$IywClawBackupDir; target_exists=0; attempts=$R2"
    StrCpy $IywClawTransactionError "无法原子备份旧 app：目录可能被占用、权限不足或文件系统拒绝"
    Push "0"
FunctionEnd

Function IywClawCleanupOrIsolateHistoricalBackup
  StrCpy $R2 1
  backup_cleanup_retry:
    ClearErrors
    RMDir /r "$IywClawBackupDir"
    IfFileExists "$IywClawBackupDir" 0 backup_cleanup_success
    IntCmp $R2 ${IYW_CLAW_BACKUP_REMOVE_ATTEMPTS} backup_cleanup_isolate backup_cleanup_wait backup_cleanup_isolate
  backup_cleanup_wait:
    DetailPrint "历史 installer backup 清理失败，等待后重试（$R2/${IYW_CLAW_BACKUP_REMOVE_ATTEMPTS}）：$IywClawBackupDir"
    IntOp $R2 $R2 + 1
    Sleep ${IYW_CLAW_BACKUP_REMOVE_WAIT_MS}
    Goto backup_cleanup_retry

  backup_cleanup_isolate:
    Call IywClawIsolateHistoricalBackup
    Pop $R0
    StrCmp $R0 "1" backup_cleanup_success 0
    IfFileExists "$IywClawBackupDir" 0 backup_cleanup_success
    StrCpy $IywClawTransactionError "无法清理或隔离历史 installer backup"
    Push "0"
    Return

  backup_cleanup_success:
    Push "1"
FunctionEnd

Function IywClawIsolateHistoricalBackup
  ${GetTime} "" "L" $R3 $R4 $R5 $R6 $R7 $R8 $R9
  StrCpy $R0 "$IywClawRoot\staging\installer-app-backup-recovery-$R5$R4$R3-$R7$R8$R9"
  StrCpy $R1 0
  backup_recovery_target:
    StrCmp $R1 0 backup_recovery_without_suffix
    StrCpy $IywClawRecoveryDir "$R0-$R1"
    Goto backup_recovery_target_checked
  backup_recovery_without_suffix:
    StrCpy $IywClawRecoveryDir "$R0"
  backup_recovery_target_checked:
    IfFileExists "$IywClawRecoveryDir" backup_recovery_next 0
    StrCpy $R2 1
  backup_recovery_rename:
    ClearErrors
    Rename "$IywClawBackupDir" "$IywClawRecoveryDir"
    IfErrors backup_recovery_rename_failed 0
    DetailPrint "历史 installer backup 无法删除，已隔离到：$IywClawRecoveryDir"
    Push "1"
    Return
  backup_recovery_rename_failed:
    IntCmp $R2 ${IYW_CLAW_RECOVERY_RENAME_ATTEMPTS} backup_recovery_next backup_recovery_rename_wait backup_recovery_next
  backup_recovery_rename_wait:
    DetailPrint "历史 installer backup 隔离失败，等待后重试（$R2/${IYW_CLAW_RECOVERY_RENAME_ATTEMPTS}）：$IywClawRecoveryDir"
    IntOp $R2 $R2 + 1
    Sleep ${IYW_CLAW_BACKUP_REMOVE_WAIT_MS}
    Goto backup_recovery_rename
  backup_recovery_next:
    IntOp $R1 $R1 + 1
    IntCmp $R1 ${IYW_CLAW_RECOVERY_TARGET_ATTEMPTS} backup_recovery_failed backup_recovery_target backup_recovery_failed
  backup_recovery_failed:
    DetailPrint "历史 installer backup 隔离失败：source=$IywClawBackupDir; target=$IywClawRecoveryDir"
    Push "0"
FunctionEnd
