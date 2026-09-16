Var IywClawAppDir
Var IywClawBackupDir
Var IywClawRequireBrowser
Var IywClawTransactionActive
Var IywClawTransactionError
Var IywClawTransactionHasBackup
Var IywClawRestartOnFailure
Var IywClawRestartArgs
Var IywClawRecoveryDir
Var IywClawAppCheckError
Var IywClawFileCheckPath
Var IywClawFileCheckReason
Var IywClawFileCheckAttempts
Var IywClawFileCheckErrorCode

!define IYW_CLAW_FILE_CHECK_ATTEMPTS 5
!define IYW_CLAW_FILE_CHECK_WAIT_MS 500

!include "${__FILEDIR__}\installer-app-backup.nsh"

Function IywClawConfigureAppTransaction
  StrCpy $IywClawAppDir "$IywClawRoot\app"
  StrCpy $IywClawBackupDir "$IywClawRoot\staging\installer-app-backup"
  StrCpy $IywClawTransactionActive "0"
  StrCpy $IywClawTransactionError ""
  StrCpy $IywClawTransactionHasBackup "0"
  StrCpy $IywClawRestartOnFailure "0"
  StrCpy $IywClawRestartArgs ""
  StrCpy $IywClawRecoveryDir ""
  StrCpy $IywClawAppCheckError ""
  ClearErrors
  ${GetOptions} $CMDLINE "/R" $R0
  IfErrors transaction_configured 0
  StrCpy $IywClawRestartOnFailure "1"
  ClearErrors
  ${GetOptions} $CMDLINE "/ARGS" $IywClawRestartArgs
  IfErrors 0 transaction_configured
  StrCpy $IywClawRestartArgs ""

  transaction_configured:
FunctionEnd

Function IywClawAppendInstallerLog
  Exch $0
  Push $1
  StrCmp $IywClawRoot "" installer_log_done
  CreateDirectory "$IywClawRoot\logs"
  ClearErrors
  FileOpen $1 "$IywClawRoot\logs\installer.log" a
  IfErrors installer_log_done 0
  FileWrite $1 "$0$\r$\n"
  FileClose $1

  installer_log_done:
    Pop $1
    Pop $0
FunctionEnd

Function IywClawCheckFileOnce
  Push $1
  Push $2
  StrCpy $0 "0"
  IfFileExists "$IywClawFileCheckPath" check_file_once_open check_file_once_failure

  check_file_once_open:
    ClearErrors
    FileOpen $1 "$IywClawFileCheckPath" r
    IfErrors check_file_once_unreadable 0
    ClearErrors
    FileSeek $1 0 END $2
    IfErrors check_file_once_seek_failed 0
    FileClose $1
    StrCpy $IywClawFileCheckErrorCode "0"
    IntCmp $2 0 check_file_once_zero check_file_once_zero check_file_once_success

  check_file_once_success:
    StrCpy $IywClawFileCheckReason "ok"
    StrCpy $0 "1"
    Goto check_file_once_done

  check_file_once_zero:
    StrCpy $IywClawFileCheckReason "文件为空（0 字节）"
    Goto check_file_once_done

  check_file_once_unreadable:
    System::Call 'kernel32::GetLastError() i.R4'
    StrCpy $IywClawFileCheckErrorCode "$R4"
    StrCpy $IywClawFileCheckReason "文件无法读取"
    StrCpy $0 "2"
    Goto check_file_once_done

  check_file_once_seek_failed:
    FileClose $1
    System::Call 'kernel32::GetLastError() i.R4'
    StrCpy $IywClawFileCheckErrorCode "$R4"
    StrCpy $IywClawFileCheckReason "无法读取文件大小"
    StrCpy $0 "2"
    Goto check_file_once_done

  check_file_once_failure:
    StrCpy $IywClawFileCheckReason "文件不存在"
    StrCpy $IywClawFileCheckErrorCode "2"
    StrCpy $0 "3"

  check_file_once_done:
    Pop $2
    Pop $1
    Push $0
FunctionEnd

Function IywClawIsNonEmptyFile
  Exch $0
  Push $1
  Push $2
  Push $3
  StrCpy $IywClawFileCheckPath "$0"
  StrCpy $IywClawFileCheckReason "文件不存在"
  StrCpy $IywClawFileCheckAttempts "0"
  StrCpy $IywClawFileCheckErrorCode "2"
  StrCpy $2 "0"
  StrCpy $3 "1"

  non_empty_file_retry:
    StrCpy $IywClawFileCheckAttempts "$3"
    Call IywClawCheckFileOnce
    Pop $1
    StrCmp $1 "1" non_empty_file_success 0
    StrCmp $1 "2" non_empty_file_retryable non_empty_file_done

  non_empty_file_retryable:
    IntCmp $3 ${IYW_CLAW_FILE_CHECK_ATTEMPTS} non_empty_file_done non_empty_file_wait non_empty_file_done
  non_empty_file_wait:
    Sleep ${IYW_CLAW_FILE_CHECK_WAIT_MS}
    IntOp $3 $3 + 1
    Goto non_empty_file_retry

  non_empty_file_success:
    StrCpy $2 "1"

  non_empty_file_done:
    StrCpy $0 $2
    Pop $3
    Pop $2
    Pop $1
    Exch $0
FunctionEnd

Function IywClawIsAppComplete
  Exch $0
  Push $1
  StrCpy $1 "0"
  Push "$0\iyw-claw.exe"
  Call IywClawIsNonEmptyFile
  Pop $1
  StrCmp $1 "1" 0 app_complete_done
  StrCmp $IywClawRequireBrowser "1" 0 app_complete_success
  Push "$0\agent-browser.exe"
  Call IywClawIsNonEmptyFile
  Pop $1
  StrCmp $1 "1" 0 app_complete_done

  app_complete_success:
    StrCpy $IywClawAppCheckError ""
    StrCpy $1 "1"
  app_complete_done:
    StrCmp $1 "1" app_complete_return
    StrCpy $IywClawAppCheckError "$IywClawFileCheckPath：$IywClawFileCheckReason（错误码=$IywClawFileCheckErrorCode，检查次数=$IywClawFileCheckAttempts）"
  app_complete_return:
    StrCpy $0 $1
    Pop $1
    Exch $0
FunctionEnd

Function IywClawValidateTransactionPaths
  StrCmp $IywClawRoot "" invalid_transaction_paths 0
  GetFullPathName $R0 "$IywClawRoot\app"
  GetFullPathName $R1 "$IywClawAppDir"
  StrCmp $R0 $R1 0 invalid_transaction_paths
  GetFullPathName $R0 "$IywClawRoot\staging\installer-app-backup"
  GetFullPathName $R1 "$IywClawBackupDir"
  StrCmp $R0 $R1 0 invalid_transaction_paths
  Push "1"
  Return

  invalid_transaction_paths:
    StrCpy $IywClawTransactionError "事务目录校验失败"
    Push "0"
FunctionEnd

Function IywClawReconcileHistoricalBackup
  IfFileExists "$IywClawBackupDir" 0 no_historical_backup
  Push "$IywClawAppDir"
  Call IywClawIsAppComplete
  Pop $R0
  Push "$IywClawBackupDir"
  Call IywClawIsAppComplete
  Pop $R1
  StrCmp $R0 "1" discard_historical_backup 0
  StrCmp $R1 "1" restore_historical_backup ambiguous_historical_state

  discard_historical_backup:
    DetailPrint "当前 app 完整，正在清理上次遗留的 installer backup..."
    Call IywClawCleanupOrIsolateHistoricalBackup
    Pop $R0
    StrCmp $R0 "1" 0 historical_cleanup_failed
    Goto no_historical_backup

  restore_historical_backup:
    DetailPrint "当前 app 不完整，正在恢复上次 installer backup..."
    RMDir /r "$IywClawAppDir"
    IfFileExists "$IywClawAppDir" historical_restore_failed 0
    ClearErrors
    Rename "$IywClawBackupDir" "$IywClawAppDir"
    IfErrors historical_restore_failed 0
    DetailPrint "上次 installer backup 已恢复。"
    Goto no_historical_backup

  ambiguous_historical_state:
    StrCpy $IywClawTransactionError "app 与历史 backup 均不完整，已保留现场"
    Push "0"
    Return
  historical_cleanup_failed:
    StrCpy $IywClawTransactionError "无法清理或隔离历史 installer backup"
    Push "0"
    Return
  historical_restore_failed:
    StrCpy $IywClawTransactionError "无法恢复历史 installer backup"
    Push "0"
    Return
  no_historical_backup:
    Push "1"
FunctionEnd

Function IywClawBeginAppTransaction
  Call IywClawValidateTransactionPaths
  Pop $R0
  StrCmp $R0 "1" 0 begin_transaction_failed
  ; ResolveInstallRoot 会把 NSIS 工作目录切到 app。重命名或删除 app 前必须
  ; 回到逻辑根目录，否则 Windows 会拒绝替换当前工作目录。
  SetOutPath "$IywClawRoot"
  Call IywClawReconcileHistoricalBackup
  Pop $R0
  StrCmp $R0 "1" 0 begin_transaction_failed
  CreateDirectory "$IywClawRoot\staging"
  IfFileExists "$IywClawAppDir\*.*" backup_current_app remove_empty_app

  backup_current_app:
    Call IywClawBackupCurrentAppWithRetry
    Pop $R2
    StrCmp $R2 "1" backup_current_app_ready begin_transaction_failed
  backup_current_app_ready:
    StrCpy $IywClawTransactionActive "1"
    StrCpy $IywClawTransactionHasBackup "1"
    DetailPrint "旧 app 已备份到 staging\installer-app-backup。"
    Goto create_new_app

  remove_empty_app:
    ClearErrors
    RMDir "$IywClawAppDir"
    IfErrors remove_empty_app_failed 0
    StrCpy $IywClawTransactionActive "1"

  create_new_app:
    ClearErrors
    CreateDirectory "$IywClawAppDir"
    IfErrors create_new_app_failed 0
    StrCpy $INSTDIR "$IywClawAppDir"
    SetOutPath "$INSTDIR"
    Push "1"
    Return

  remove_empty_app_failed:
    StrCpy $IywClawTransactionError "无法移除旧的空 app 目录"
    Goto begin_transaction_failed
  create_new_app_failed:
    StrCpy $IywClawTransactionError "无法创建新的 app 目录"
  begin_transaction_failed:
    DetailPrint "app 事务启动失败：$IywClawTransactionError"
    Push "begin: $IywClawTransactionError"
    Call IywClawAppendInstallerLog
    Push "0"
FunctionEnd

Function IywClawCommitAppTransaction
  StrCmp $IywClawTransactionActive "1" 0 inactive_transaction
  Call IywClawValidateTransactionPaths
  Pop $R0
  StrCmp $R0 "1" 0 commit_transaction_failed
  Push "$IywClawAppDir"
  Call IywClawIsAppComplete
  Pop $R0
  StrCmp $R0 "1" app_install_valid 0
  StrCpy $IywClawTransactionError "新 app 校验失败：$IywClawAppCheckError"
  Goto commit_transaction_failed

  app_install_valid:
    Push "$IywClawAppDir"
    Push "check-legacy-files"
    Call IywClawRunKnownProcessCommandAt
    Pop $R0
    StrCmp $R0 "0" app_legacy_check_complete 0
    StrCmp $R0 "1" app_legacy_found app_legacy_check_failed

  app_legacy_found:
    StrCpy $IywClawTransactionError "新 app 仍包含旧 iyw-claw-mcp 文件"
    Goto commit_transaction_failed
  app_legacy_check_failed:
    StrCpy $IywClawTransactionError "无法复核新 app 的旧 MCP 文件"
    Goto commit_transaction_failed

  app_legacy_check_complete:
    ; 新 app 完整即成为可恢复版本。backup 删除不是原子操作，优先清理，失败时
    ; 隔离到 recovery 目录；只有清理和隔离都失败才阻断成功。
    StrCpy $IywClawTransactionActive "0"
    Call IywClawCleanupOrIsolateHistoricalBackup
    Pop $R0
    StrCmp $R0 "1" 0 backup_cleanup_failed
    Push "$IywClawBackupDir"
    Push "check-legacy-files"
    Call IywClawRunKnownProcessCommandAt
    Pop $R0
    StrCmp $R0 "0" backup_legacy_check_complete 0
    StrCmp $R0 "1" backup_legacy_found backup_legacy_check_failed

  backup_legacy_found:
    StrCpy $IywClawTransactionError "installer backup 仍包含旧 iyw-claw-mcp 文件"
    Goto commit_transaction_failed
  backup_legacy_check_failed:
    StrCpy $IywClawTransactionError "无法复核 installer backup 的旧 MCP 文件"
    Goto commit_transaction_failed

  backup_legacy_check_complete:
    IfFileExists "$IywClawBackupDir\*.*" backup_cleanup_failed 0
    IfFileExists "$IywClawBackupDir" backup_cleanup_failed 0
    Goto commit_transaction_done

  backup_cleanup_failed:
    StrCpy $IywClawTransactionError "新 app 已完整，但 installer backup 清理或隔离未完成"
    Goto commit_transaction_failed

  inactive_transaction:
    StrCpy $IywClawTransactionError "app 事务未启动"

  commit_transaction_failed:
    DetailPrint "app 事务提交失败：$IywClawTransactionError"
    Push "commit: $IywClawTransactionError"
    Call IywClawAppendInstallerLog
    Push "0"
    Return

  commit_transaction_done:
    StrCpy $IywClawTransactionHasBackup "0"
    DetailPrint "新 app 校验完成，app 事务已提交。"
    Push "1"
FunctionEnd

Function IywClawRollbackAppTransaction
  StrCmp $IywClawTransactionActive "1" 0 rollback_not_required
  Call IywClawValidateTransactionPaths
  Pop $R0
  StrCmp $R0 "1" 0 rollback_failed
  SetOutPath "$IywClawRoot"
  DetailPrint "安装失败，正在恢复旧 app..."
  RMDir /r "$IywClawAppDir"
  IfFileExists "$IywClawAppDir" rollback_failed 0
  IfFileExists "$IywClawBackupDir\*.*" restore_transaction_backup rollback_without_backup

  restore_transaction_backup:
    ClearErrors
    Rename "$IywClawBackupDir" "$IywClawAppDir"
    IfErrors rollback_failed 0
    Goto rollback_done

  rollback_without_backup:
    CreateDirectory "$IywClawAppDir"

  rollback_done:
    StrCpy $IywClawTransactionActive "0"
    StrCpy $INSTDIR "$IywClawAppDir"
    Push "1"
    Return

  rollback_failed:
    DetailPrint "旧 app 恢复失败，installer backup 已保留：$IywClawBackupDir"
    Push "0"
    Return
  rollback_not_required:
    Push "1"
FunctionEnd

Function IywClawRestartOldAppIfRequested
  StrCmp $IywClawRestartOnFailure "1" 0 restart_old_app_done
  ; 旧版本可能早于 browser sidecar 完整性规则；回滚重启只要求主程序可用。
  Push "$IywClawAppDir\iyw-claw.exe"
  Call IywClawIsNonEmptyFile
  Pop $R0
  StrCmp $R0 "1" 0 restart_old_app_done
  Push "check-main"
  Call IywClawRunKnownProcessCommand
  Pop $R0
  ; The process helper returns 0=absent, 1=found, 2=check failed.
  StrCmp $R0 "0" 0 restart_old_app_done
  DetailPrint "正在重新启动旧版本 iyw-claw..."
  ; Hooks are parsed before Tauri's additional plugin directory is registered.
  ; ShellExecute keeps this current-user installer flow out of the plugin path
  ; and lets the Windows shell launch the restored app without blocking rollback.
  ExecShell "open" "$IywClawAppDir\iyw-claw.exe" "$IywClawRestartArgs"

  restart_old_app_done:
FunctionEnd
