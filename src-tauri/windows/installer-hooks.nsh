; Capture source paths while NSIS includes this file.
!define IYW_CLAW_INSTALL_REGISTRY_KEY "Software\iywclaw\iyw-claw"
!define MUI_CUSTOMFUNCTION_GUIINIT IywClawRestoreLogicalInstallRoot

Var IywClawRoot

Function IywClawRestoreLogicalInstallRoot
  ; Older installers persisted root\app as MUI's default directory while the
  ; product-specific InstallRoot value already held the user-selected root.
  ; Correct only the directory-page value; POSTINSTALL persists it after the
  ; old uninstaller has finished using its internal root\app working directory.
  ReadRegStr $R8 SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "InstallRoot"
  StrCmp $R8 "" iyw_guiinit_done 0
  GetFullPathName $R9 "$R8\app"
  GetFullPathName $R7 "$INSTDIR"
  StrCmp $R7 $R9 0 iyw_guiinit_done
  GetFullPathName $INSTDIR "$R8"

  iyw_guiinit_done:
    ; A regular installer launch would show Tauri's reinstall choice page.
    ; Relaunch an existing installation in passive update mode so the current
    ; directory is reused and no uninstaller UI is shown.
    ClearErrors
    ${GetOptions} $CMDLINE "/UPDATE" $R6
    IfErrors 0 iyw_guiinit_return
    ReadRegStr $R8 SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "InstallRoot"
    StrCmp $R8 "" iyw_guiinit_return 0
    ExecWait '"$EXEPATH" /P /UPDATE' $R6
    SetErrorLevel $R6
    Quit

  iyw_guiinit_return:
FunctionEnd

Function IywClawResolveInstallRoot
  ReadRegStr $R8 SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "InstallRoot"
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
    CreateDirectory "$IywClawRoot"
    ClearErrors
    FileOpen $R0 "$IywClawRoot\.iyw-claw-install-probe" w
    IfErrors iyw_invalid_root
    FileWrite $R0 "iyw-claw"
    FileClose $R0
    Delete "$IywClawRoot\.iyw-claw-install-probe"

    CreateDirectory "$IywClawRoot\app"
    CreateDirectory "$IywClawRoot\runtime"
    CreateDirectory "$IywClawRoot\runtime\downloads"
    CreateDirectory "$IywClawRoot\runtime\staging"
    CreateDirectory "$IywClawRoot\runtime\trash"
    CreateDirectory "$IywClawRoot\config"
    CreateDirectory "$IywClawRoot\data"
    CreateDirectory "$IywClawRoot\logs"

    StrCpy $INSTDIR "$IywClawRoot\app"
    SetOutPath "$INSTDIR"
    WriteRegStr SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "InstallRoot" "$IywClawRoot"
    DetailPrint "安装目录：$IywClawRoot"
    Return

  iyw_invalid_root:
    MessageBox MB_OK|MB_ICONSTOP \
      "无法写入所选安装目录：$IywClawRoot$\r$\n请返回并选择其他目录。"
    Abort
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  Call IywClawResolveInstallRoot
  DetailPrint "正在停止运行中的 iyw-claw 后台进程..."
  nsExec::Exec 'taskkill /F /T /IM iyw-claw.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /T /IM iyw-claw-mcp.exe'
  Pop $0
  Sleep 500

  ${If} $UpdateMode = 1
    DetailPrint "正在替换 iyw-claw 应用文件..."
    RMDir /r "$IywClawRoot\app"
    CreateDirectory "$IywClawRoot\app"
    StrCpy $INSTDIR "$IywClawRoot\app"
    SetOutPath "$INSTDIR"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Tauri persists the internal app directory as the next installer location.
  ; Expose the logical root in the directory page while keeping binaries
  ; isolated below root\app.
  WriteRegStr SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "" "$IywClawRoot"

  ${If} $UpdateMode = 1
    DetailPrint "已保留现有运行环境、配置、数据和日志。"
  ${Else}
    ; Node.js/npm 与 Git 运行环境不再由安装器打包安装：应用首次启动时会在
    ; 初始化界面通过国内加速镜像自动下载（见 commands/runtime_bootstrap.rs），
    ; 已存在的环境则直接复用，安装器只负责准备好 runtime 目录结构。
    DetailPrint "基础运行环境将在应用首次启动时自动准备。"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "正在停止运行中的 iyw-claw 后台进程..."
  nsExec::Exec 'taskkill /F /T /IM iyw-claw-mcp.exe'
  Pop $0
  Sleep 500

  ${If} $UpdateMode = 1
    Goto iyw_uninstall_done
  ${EndIf}

  ReadRegStr $IywClawRoot SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "InstallRoot"
  StrCmp $IywClawRoot "" iyw_uninstall_done 0
  GetFullPathName $R8 "$IywClawRoot\app"
  GetFullPathName $R9 "$INSTDIR"
  StrCmp $R8 $R9 iyw_remove_managed_dirs 0
  GetFullPathName $R7 "$IywClawRoot"
  StrCmp $R9 $R7 iyw_uninstall_from_root iyw_uninstall_done

  iyw_uninstall_from_root:
    StrCpy $INSTDIR "$IywClawRoot\app"
    SetOutPath "$INSTDIR"

  iyw_remove_managed_dirs:
    DetailPrint "正在删除可重建的私有运行环境..."
    RMDir /r "$IywClawRoot\runtime"
    RMDir /r "$IywClawRoot\logs"
    DeleteRegKey SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}"
    DetailPrint "用户配置和本地数据已保留。"

  iyw_uninstall_done:
!macroend
