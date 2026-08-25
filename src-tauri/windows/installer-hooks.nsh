; Capture source paths while NSIS includes this file.
!define IYW_CLAW_INSTALL_REGISTRY_KEY "Software\iywclaw\iyw-claw"
!define MUI_CUSTOMFUNCTION_GUIINIT IywClawRestoreLogicalInstallRoot
Var IywClawRoot
Var IywClawInstallRegistryKey

!include "${__FILEDIR__}\installer-process-control.nsh"
!include "${__FILEDIR__}\installer-app-transaction.nsh"
!include "${__FILEDIR__}\installer-test-mode.nsh"

Function IywClawIsMainProcessRunning
  ; 仅检查当前用户、本安装目录且真实路径匹配的主进程。
  ; 原样返回 helper 状态：1=运行中，0=未运行，其他=查询失败。
  Pop $R9
  Push $R9
  Push "check-main"
  Call IywClawRunKnownProcessCommandAt
  Pop $R5
  Push $R5
FunctionEnd

Function IywClawRestoreLogicalInstallRoot
  Call IywClawConfigureInstallerMode
  StrCmp $IywClawInstallerTestMode "invalid" 0 iyw_installer_mode_valid
  DetailPrint "测试模式参数无效，安装已取消。"
  SetErrorLevel 2
  Quit

  iyw_installer_mode_valid:
  ; Older installers persisted root\app as MUI's default directory while the
  ; product-specific InstallRoot value already held the user-selected root.
  ; Correct only the directory-page value; POSTINSTALL persists it after the
  ; old uninstaller has finished using its internal root\app working directory.
  ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  StrCmp $R8 "" iyw_set_default_install_root 0
  GetFullPathName $R9 "$R8\app"
  GetFullPathName $R7 "$INSTDIR"
  StrCmp $R7 $R9 0 iyw_guiinit_done
  GetFullPathName $INSTDIR "$R8"
  Goto iyw_guiinit_done

  ; Keep the displayed product name localized, but use an ASCII-only default
  ; installation root so bundled command-line tools never inherit a Chinese
  ; executable path. Users can still choose another directory in the installer.
  iyw_set_default_install_root:
    ; NSIS passes a disposable CI install root through /D=. Preserve it when
    ; present; only interactive installs should receive the product default.
    ClearErrors
    ${GetOptions} $CMDLINE "/D=" $R8
    IfErrors iyw_set_product_default_root 0
    StrCpy $INSTDIR "$R8"
    Goto iyw_guiinit_done

  iyw_set_product_default_root:
    GetFullPathName $INSTDIR "$LOCALAPPDATA\iyw-claw"

  iyw_guiinit_done:
    ; 测试模式不得检查或重启生产进程；应用内 updater 已携带 /UPDATE 时
    ; 继续当前 NSIS 流程，避免递归启动安装器。
    StrCmp $IywClawInstallerTestMode "1" iyw_guiinit_return 0
    ClearErrors
    ${GetOptions} $CMDLINE "/UPDATE" $R6
    IfErrors 0 iyw_guiinit_return
    ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
    StrCmp $R8 "" iyw_guiinit_return 0

    ; 旧版卸载可能已经删除 app，但注册表项尚未完成清理。此时没有
    ; 可替换的旧主程序，按已卸载残留继续；存在主程序时仍严格检查身份。
    IfFileExists "$R8\app\iyw-claw.exe" 0 iyw_relaunch_stopped_app
    Push "$R8\app"
    Call IywClawIsMainProcessRunning
    Pop $R5
    StrCmp $R5 "1" iyw_relaunch_running_app 0
    StrCmp $R5 "0" iyw_relaunch_stopped_app iyw_relaunch_query_failed

  iyw_relaunch_stopped_app:
    ExecWait '"$EXEPATH" /P /UPDATE' $R6
    Goto iyw_relaunch_finished

  iyw_relaunch_running_app:
    DetailPrint "检测到正在运行的 iyw-claw，安装完成后将恢复原页面。"
    ExecWait '"$EXEPATH" /P /R /UPDATE /ARGS --restore-installer-session' $R6
    Goto iyw_relaunch_finished

  iyw_relaunch_query_failed:
    DetailPrint "无法确认 iyw-claw 主进程状态，已停止更新。"
    IfSilent iyw_relaunch_query_failed_quit 0
    MessageBox MB_OK|MB_ICONSTOP \
      "无法确认当前用户的 iyw-claw 是否正在运行。为避免不安全替换，本次更新已停止。"
  iyw_relaunch_query_failed_quit:
    SetErrorLevel 1
    Quit

  iyw_relaunch_finished:
    SetErrorLevel $R6
    Quit

  iyw_guiinit_return:
FunctionEnd

Function IywClawResolveInstallRoot
  ReadRegStr $R8 SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  StrCmp $R8 "" iyw_use_selected_root 0

  GetFullPathName $R9 "$R8\app"
  GetFullPathName $R7 "$INSTDIR"
  StrCmp $R7 $R9 iyw_use_stored_root iyw_use_selected_root

  iyw_use_stored_root:
    StrCpy $IywClawRoot $R8
    Goto iyw_validate_root

  iyw_use_selected_root:
    StrCpy $IywClawRoot $INSTDIR

  iyw_validate_root:
    StrCmp $IywClawRoot "" iyw_invalid_root 0
    GetFullPathName $IywClawRoot "$IywClawRoot"
    StrCmp $IywClawInstallerTestMode "1" 0 iyw_root_scope_validated
    Call IywClawValidateTestRoot
    Pop $R0
    StrCmp $R0 "1" iyw_root_scope_validated 0
    DetailPrint "测试安装目录必须位于绑定的临时 smoke 根目录。"
    Abort

  iyw_root_scope_validated:
    CreateDirectory "$IywClawRoot"
    ClearErrors
    FileOpen $R0 "$IywClawRoot\.iyw-claw-install-probe" w
    IfErrors iyw_invalid_root
    FileWrite $R0 "iyw-claw"
    FileClose $R0
    Delete "$IywClawRoot\.iyw-claw-install-probe"

    ; 持久区布局：app 是唯一会被应用更新替换的区域；runtime/agents/skills/
    ; inventory/staging 为受管内容（由版本中心初始化与激活）；config/data/logs
    ; 为现有持久区。更新永不清理这些目录。
    CreateDirectory "$IywClawRoot\app"
    CreateDirectory "$IywClawRoot\runtime"
    CreateDirectory "$IywClawRoot\agents"
    CreateDirectory "$IywClawRoot\skills"
    CreateDirectory "$IywClawRoot\inventory"
    CreateDirectory "$IywClawRoot\staging"
    CreateDirectory "$IywClawRoot\config"
    CreateDirectory "$IywClawRoot\data"
    CreateDirectory "$IywClawRoot\logs"

    StrCpy $INSTDIR "$IywClawRoot\app"
    SetOutPath "$INSTDIR"
    WriteRegStr SHCTX "$IywClawInstallRegistryKey" "InstallRoot" "$IywClawRoot"
    DetailPrint "安装目录：$IywClawRoot"
    Return

  iyw_invalid_root:
    MessageBox MB_OK|MB_ICONSTOP \
      "无法写入所选安装目录：$IywClawRoot$\r$\n请返回并选择其他目录。"
    Abort
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  Call IywClawConfigureInstallerMode
  StrCmp $IywClawInstallerTestMode "invalid" iyw_invalid_install_test_mode 0
  StrCmp $IywClawInstallerTestMode "1" 0 iyw_test_root_prevalidated
  ; 在 ResolveInstallRoot 创建目录或写入注册表前，先约束 /D 到本轮测试根。
  StrCpy $IywClawRoot "$INSTDIR"
  Call IywClawValidateTestRoot
  Pop $R0
  StrCmp $R0 "1" iyw_test_root_prevalidated 0
  DetailPrint "测试安装目录必须位于绑定的临时 smoke 根目录。"
  Abort

  iyw_test_root_prevalidated:
  Call IywClawResolveInstallRoot
  StrCmp $IywClawInstallerTestMode "1" 0 iyw_test_root_validated
  Call IywClawValidateTestRoot
  Pop $R0
  StrCmp $R0 "1" iyw_test_root_validated 0
  DetailPrint "测试安装目录必须位于绑定的临时 smoke 根目录。"
  Abort

  iyw_test_root_validated:
  !if "${ARCH}" == "x64"
    StrCpy $IywClawRequireBrowser "1"
  !else
    StrCpy $IywClawRequireBrowser "0"
  !endif
  Call IywClawConfigureAppTransaction
  StrCmp $IywClawInstallerTestMode "1" iyw_begin_app_transaction 0
  Call IywClawStopKnownProcesses
  Pop $R0
  StrCmp $R0 "0" iyw_begin_app_transaction 0
  IfSilent iyw_abort_install_for_processes 0
  ${If} $PassiveMode != 1
    MessageBox MB_OK|MB_ICONSTOP \
      "无法停止当前用户的后台进程：$IywClawProcessError$\r$\n安装尚未修改旧版本。"
  ${EndIf}
  iyw_abort_install_for_processes:
    Abort

  iyw_begin_app_transaction:
    Call IywClawBeginAppTransaction
    Pop $R0
    StrCmp $R0 "1" iyw_app_transaction_ready 0
    IfSilent iyw_abort_install_for_transaction 0
    ${If} $PassiveMode != 1
      MessageBox MB_OK|MB_ICONSTOP \
        "无法安全替换应用目录：$IywClawTransactionError$\r$\n旧版本和备份现场已保留。"
    ${EndIf}
    iyw_abort_install_for_transaction:
      Abort

  iyw_app_transaction_ready:
  Goto iyw_preinstall_done

  iyw_invalid_install_test_mode:
    DetailPrint "测试模式参数无效，安装尚未修改任何文件。"
    Abort

  iyw_preinstall_done:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  Call IywClawCommitAppTransaction
  Pop $R0
  StrCmp $R0 "1" iyw_app_transaction_committed 0
  Abort

  iyw_app_transaction_committed:
  ; Tauri persists the internal app directory as the next installer location.
  ; Expose the logical root in the directory page while keeping binaries
  ; isolated below root\app.
  WriteRegStr SHCTX "$IywClawInstallRegistryKey" "" "$IywClawRoot"

  ${If} $UpdateMode = 1
    DetailPrint "已保留运行环境、受管组件、配置、数据和日志。"
  ${Else}
    ; 安装包已包含基础运行时种子，首次启动校验后导入，失败才在线回退。
    DetailPrint "首次启动将校验并导入基础运行时种子，失败后才在线回退。"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Call un.IywClawConfigureInstallerMode
  StrCmp $IywClawInstallerTestMode "invalid" iyw_invalid_uninstall_test_mode 0
  StrCmp $IywClawInstallerTestMode "1" 0 iyw_uninstall_test_root_validated
  ReadRegStr $IywClawRoot SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  StrCmp $IywClawRoot "" iyw_invalid_uninstall_test_mode 0
  Call un.IywClawValidateTestRoot
  Pop $R0
  StrCmp $R0 "1" iyw_uninstall_test_root_validated 0
  DetailPrint "测试卸载目录必须位于绑定的临时 smoke 根目录。"
  Abort

  iyw_uninstall_test_root_validated:
  StrCmp $IywClawInstallerTestMode "1" iyw_uninstall_processes_stopped 0
  Call un.IywClawStopKnownProcesses
  Pop $R0
  StrCmp $R0 "0" iyw_uninstall_processes_stopped 0
  IfSilent iyw_abort_uninstall_for_processes 0
  ${If} $PassiveMode != 1
    MessageBox MB_OK|MB_ICONSTOP \
      "无法停止当前用户的后台进程：$IywClawProcessError$\r$\n卸载尚未修改应用文件。"
  ${EndIf}
  iyw_abort_uninstall_for_processes:
    Abort

  iyw_uninstall_processes_stopped:

  ${If} $UpdateMode = 1
    Goto iyw_uninstall_done
  ${EndIf}

  ReadRegStr $IywClawRoot SHCTX "$IywClawInstallRegistryKey" "InstallRoot"
  StrCmp $IywClawRoot "" iyw_uninstall_done 0
  GetFullPathName $IywClawRoot "$IywClawRoot"

  ; 更新由 Tauri 以 /UPDATE 启动，已在上方直接结束，不进入用户数据确认。
  ; 普通交互式卸载询问是否保留用户数据：保留时删除所有程序目录，只留下
  ; config/data/skills；/PURGE 用于静默或自动化场景，明确删除整个安装根目录。
  ${If} $CmdLine == *"/PURGE"*
    Goto iyw_purge_all_user_data
  ${EndIf}

  IfSilent iyw_keep_user_data 0
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "是否保留用户数据？$\r$\n选择“是”保留配置、数据和 Skill；选择“否”删除全部本地数据。" IDYES iyw_keep_user_data
  Goto iyw_purge_all_user_data

  iyw_keep_user_data:
    DetailPrint "正在删除 iyw-claw 程序数据并保留用户数据..."
    RMDir /r "$IywClawRoot\app"
    RMDir /r "$IywClawRoot\runtime"
    RMDir /r "$IywClawRoot\agents"
    RMDir /r "$IywClawRoot\inventory"
    RMDir /r "$IywClawRoot\staging"
    RMDir /r "$IywClawRoot\plugins"
    RMDir /r "$IywClawRoot\logs"
    DeleteRegKey SHCTX "$IywClawInstallRegistryKey"
    DetailPrint "程序数据已删除，用户配置、本地数据与 Skill 已保留。"
    Goto iyw_uninstall_done

  iyw_purge_all_user_data:
    GetFullPathName $R8 "$IywClawRoot"
    StrCmp $R8 "" iyw_uninstall_done 0
    DetailPrint "彻底删除模式：正在移除全部安装内容..."
    RMDir /r "$R8"
    DeleteRegKey SHCTX "$IywClawInstallRegistryKey"
    Goto iyw_uninstall_done

  iyw_uninstall_done:
  Goto iyw_preuninstall_done

  iyw_invalid_uninstall_test_mode:
    DetailPrint "测试模式参数无效，卸载尚未修改任何文件。"
    Abort

  iyw_preuninstall_done:
!macroend

Function .onInstFailed
  Call IywClawRollbackAppTransaction
  Pop $R0
  Call IywClawRestartOldAppIfRequested
FunctionEnd
