!define IYW_CLAW_APP_RENAME_ATTEMPTS 5
!define IYW_CLAW_APP_RENAME_WAIT_MS 500
!define IYW_CLAW_BACKUP_REMOVE_ATTEMPTS 5
!define IYW_CLAW_BACKUP_REMOVE_WAIT_MS 500
!define IYW_CLAW_RECOVERY_TARGET_ATTEMPTS 10
!define IYW_CLAW_RECOVERY_RENAME_ATTEMPTS 3

Var IywClawAppRenameError
Var IywClawAppLockChecked
!include "${__FILEDIR__}\installer-app-locks.nsh"
!include "${__FILEDIR__}\installer-app-content.nsh"

Function IywClawBackupCurrentAppWithRetry
  backup_current_app_restart:
  StrCpy $IywClawTransactionError ""
  StrCpy $IywClawAppLockChecked "0"
  StrCpy $R2 1
  backup_current_app_retry:
    ClearErrors
    ; 直接保留 MoveFileW 的错误码，避免 NSIS Rename 只留下通用失败标记。
    System::Call 'kernel32::MoveFileW(w "$IywClawAppDir", w "$IywClawBackupDir") i.R0 ?e'
    Pop $IywClawAppRenameError
    StrCmp $R0 "0" backup_current_app_retry_failed 0
    Push "1"
    Return

  backup_current_app_retry_failed:
    IfFileExists "$IywClawBackupDir" backup_current_app_target_exists 0
    IntCmp $R2 ${IYW_CLAW_APP_RENAME_ATTEMPTS} backup_current_app_failed backup_current_app_wait backup_current_app_failed
  backup_current_app_wait:
    DetailPrint "旧 app 原子备份失败，等待后重试（$R2/${IYW_CLAW_APP_RENAME_ATTEMPTS}）：$IywClawAppDir -> $IywClawBackupDir; win32_error=$IywClawAppRenameError"
    IntOp $R2 $R2 + 1
    Sleep ${IYW_CLAW_APP_RENAME_WAIT_MS}
    Goto backup_current_app_retry

  backup_current_app_target_exists:
    DetailPrint "旧 app 原子备份失败：source=$IywClawAppDir; target=$IywClawBackupDir; target_exists=1; attempts=$R2; win32_error=$IywClawAppRenameError"
    StrCpy $IywClawTransactionError "无法原子备份旧 app：backup 目标已存在"
    Push "0"
    Return
  backup_current_app_failed:
    DetailPrint "旧 app 原子备份失败：source=$IywClawAppDir; target=$IywClawBackupDir; target_exists=0; attempts=$R2; win32_error=$IywClawAppRenameError"
    StrCpy $IywClawTransactionError "无法原子备份旧 app：Windows 错误 $IywClawAppRenameError"
    StrCmp $IywClawAppRenameError "5" backup_current_app_inspect 0
    StrCmp $IywClawAppRenameError "32" backup_current_app_inspect backup_current_app_return_failed
  backup_current_app_inspect:
    ; 子目录被资源管理器或终端持有时，父目录改名也会返回错误 5。
    StrCmp $IywClawAppLockChecked "1" backup_current_app_prompt 0
    StrCpy $IywClawAppLockChecked "1"
    Call IywClawReportAppLocks
    StrCpy $R2 1
    Sleep ${IYW_CLAW_APP_RENAME_WAIT_MS}
    Goto backup_current_app_retry
  backup_current_app_prompt:
    StrCpy $IywClawTransactionError "$IywClawTransactionError。app 或其子目录可能被占用，也可能缺少目录权限。请关闭该目录下的资源管理器窗口、终端及文件预览后重试。"
    IfSilent backup_current_app_return_failed 0
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$IywClawTransactionError$\r$\n$\r$\n目录：$IywClawAppDir$\r$\n旧版本尚未替换。关闭占用后点击重试；取消则保留旧版本并停止安装。" IDRETRY backup_current_app_restart
  backup_current_app_return_failed:
    Push "0"
FunctionEnd

Function IywClawCleanupOrIsolateHistoricalBackup
  IfFileExists "$IywClawBackupDir" 0 backup_cleanup_success
  Call IywClawInspectBackupContent
  Pop $R0
  StrCmp $R0 "0" backup_cleanup_begin 0
  StrCmp $R0 "1" backup_cleanup_isolate 0
  StrCpy $IywClawTransactionError "无法确认旧 app 内容，已保留备份并停止清理"
  Push "0"
  Return
  backup_cleanup_begin:
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
    DetailPrint "旧 app 备份已保留到恢复目录：$IywClawRecoveryDir"
    Push "retained app backup: $IywClawRecoveryDir"
    Call IywClawAppendInstallerLog
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
