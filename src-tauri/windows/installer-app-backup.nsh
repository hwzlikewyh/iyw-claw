!define IYW_CLAW_APP_RENAME_ATTEMPTS 5
!define IYW_CLAW_APP_RENAME_WAIT_MS 500

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
