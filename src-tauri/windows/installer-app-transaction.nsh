Var IywClawAppDir
Var IywClawBackupDir
Var IywClawRequireBrowser
Var IywClawTransactionActive
Var IywClawTransactionError
Var IywClawRestartOnFailure
Var IywClawRestartArgs

Function IywClawConfigureAppTransaction
  StrCpy $IywClawAppDir "$IywClawRoot\app"
  StrCpy $IywClawBackupDir "$IywClawRoot\staging\installer-app-backup"
  StrCpy $IywClawTransactionActive "0"
  StrCpy $IywClawTransactionError ""
  StrCpy $IywClawRestartOnFailure "0"
  StrCpy $IywClawRestartArgs ""
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

Function IywClawIsNonEmptyFile
  Exch $0
  Push $1
  Push $2
  StrCpy $2 "0"
  ClearErrors
  FileOpen $1 "$0" r
  IfErrors non_empty_file_done 0
  FileSeek $1 0 END $0
  FileClose $1
  IntCmp $0 0 non_empty_file_done non_empty_file_done 0
  StrCpy $2 "1"

  non_empty_file_done:
    StrCpy $0 $2
    Pop $2
    Pop $1
    Exch $0
FunctionEnd

Function IywClawHasVersionedMcp
  Exch $0
  Push $1
  Push $2
  Push $3
  StrCpy $3 "0"
  FindFirst $1 $2 "$0\iyw-claw-mcp-*.exe"

  versioned_mcp_loop:
    StrCmp $2 "" versioned_mcp_done 0
    Push "$0\$2"
    Call IywClawIsNonEmptyFile
    Pop $3
    StrCmp $3 "1" versioned_mcp_done 0
    FindNext $1 $2
    Goto versioned_mcp_loop

  versioned_mcp_done:
    FindClose $1
    StrCpy $0 $3
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
  Push "$0\iyw-claw-mcp.exe"
  Call IywClawIsNonEmptyFile
  Pop $1
  StrCmp $1 "1" 0 app_complete_done
  Push "$0"
  Call IywClawHasVersionedMcp
  Pop $1
  StrCmp $1 "1" 0 app_complete_done
  StrCmp $IywClawRequireBrowser "1" 0 app_complete_success
  Push "$0\agent-browser.exe"
  Call IywClawIsNonEmptyFile
  Pop $1
  StrCmp $1 "1" 0 app_complete_done

  app_complete_success:
    StrCpy $1 "1"
  app_complete_done:
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
  IfFileExists "$IywClawBackupDir\*.*" 0 no_historical_backup
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
    RMDir /r "$IywClawBackupDir"
    IfFileExists "$IywClawBackupDir\*.*" historical_cleanup_failed 0
    Goto no_historical_backup

  restore_historical_backup:
    DetailPrint "当前 app 不完整，正在恢复上次 installer backup..."
    RMDir /r "$IywClawAppDir"
    IfFileExists "$IywClawAppDir\*.*" historical_restore_failed 0
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
    StrCpy $IywClawTransactionError "无法清理历史 installer backup"
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
  Call IywClawReconcileHistoricalBackup
  Pop $R0
  StrCmp $R0 "1" 0 begin_transaction_failed
  CreateDirectory "$IywClawRoot\staging"
  IfFileExists "$IywClawAppDir\*.*" backup_current_app remove_empty_app

  backup_current_app:
    ClearErrors
    Rename "$IywClawAppDir" "$IywClawBackupDir"
    IfErrors backup_current_app_failed 0
    StrCpy $IywClawTransactionActive "1"
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

  backup_current_app_failed:
    StrCpy $IywClawTransactionError "无法原子备份旧 app"
    Goto begin_transaction_failed
  remove_empty_app_failed:
    StrCpy $IywClawTransactionError "无法移除旧的空 app 目录"
    Goto begin_transaction_failed
  create_new_app_failed:
    StrCpy $IywClawTransactionError "无法创建新的 app 目录"
  begin_transaction_failed:
    DetailPrint "app 事务启动失败：$IywClawTransactionError"
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
  StrCpy $IywClawTransactionError "新 app 缺少必需可执行文件"
  Goto commit_transaction_failed

  app_install_valid:
    ; 新 app 完整即成为可恢复版本。backup 删除不是原子操作，不能在部分删除后
    ; 再用它覆盖完整新 app；清理失败时保留余项，由下次安装先行清理。
    StrCpy $IywClawTransactionActive "0"
    RMDir /r "$IywClawBackupDir"
    IfFileExists "$IywClawBackupDir\*.*" backup_cleanup_deferred 0
    Goto commit_transaction_done

  backup_cleanup_deferred:
    DetailPrint "新 app 已完整；installer backup 清理未完成，将在下次安装重试。"
    Goto commit_transaction_done

  inactive_transaction:
    StrCpy $IywClawTransactionError "app 事务未启动"

  commit_transaction_failed:
    DetailPrint "app 事务提交失败：$IywClawTransactionError"
    Push "0"
    Return

  commit_transaction_done:
    DetailPrint "新 app 校验完成，app 事务已提交。"
    Push "1"
FunctionEnd

Function IywClawRollbackAppTransaction
  StrCmp $IywClawTransactionActive "1" 0 rollback_not_required
  Call IywClawValidateTransactionPaths
  Pop $R0
  StrCmp $R0 "1" 0 rollback_failed
  DetailPrint "安装失败，正在恢复旧 app..."
  RMDir /r "$IywClawAppDir"
  IfFileExists "$IywClawAppDir\*.*" rollback_failed 0
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
    DetailPrint "旧 app 已恢复。"
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
  ; 旧版本可能早于 MCP/browser sidecar 完整性规则；回滚重启只要求主程序可用。
  Push "$IywClawAppDir\iyw-claw.exe"
  Call IywClawIsNonEmptyFile
  Pop $R0
  StrCmp $R0 "1" 0 restart_old_app_done
  Call IywClawBuildAccountFilter
  Push "iyw-claw.exe"
  Call IywClawFindCurrentUserProcess
  Pop $R0
  StrCmp $R0 "2" 0 restart_old_app_done
  DetailPrint "正在重新启动旧版本 iyw-claw..."
  ; The helper plugin is unavailable while hooks are parsed. The rollback
  ; path is best-effort and can launch through the current installer token.
  Exec '"$IywClawAppDir\iyw-claw.exe" $IywClawRestartArgs'

  restart_old_app_done:
FunctionEnd
