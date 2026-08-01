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
    GetFullPathName $INSTDIR "$LOCALAPPDATA\iyw-claw"

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
  nsExec::Exec 'taskkill /F /T /IM "iyw-claw-mcp*.exe" /FI "STATUS eq RUNNING"'
  Pop $0
  Sleep 500

  ${If} $UpdateMode = 1
    DetailPrint "正在替换 iyw-claw 应用文件..."
    ; 更新只替换 app 区。任何删除路径必须先 canonicalize 并证明目标就是
    ; $IywClawRoot\app，绝不触碰 runtime/agents/skills/inventory/config/data/logs。
    StrCmp $IywClawRoot "" iyw_skip_app_replace 0
    GetFullPathName $R0 "$IywClawRoot\app"
    GetFullPathName $R1 "$INSTDIR"
    StrCmp $R0 $R1 0 iyw_skip_app_replace
    RMDir /r "$R0"
    CreateDirectory "$R0"
    StrCpy $INSTDIR "$R0"
    SetOutPath "$INSTDIR"
  iyw_skip_app_replace:
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Tauri persists the internal app directory as the next installer location.
  ; Expose the logical root in the directory page while keeping binaries
  ; isolated below root\app.
  WriteRegStr SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "" "$IywClawRoot"

  ${If} $UpdateMode = 1
    DetailPrint "已保留运行环境、受管组件、配置、数据和日志。"
  ${Else}
    ; Node.js、Git、uv、Skill 与 Agent CLI 不再随安装包附带：首次启动时由
    ; 桌面初始化流程按后端版本中心计划下载并原子激活。
    DetailPrint "首次启动将按托管分发计划初始化运行环境，不会占用安装包体积。"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "正在停止运行中的 iyw-claw 后台进程..."
  nsExec::Exec 'taskkill /F /T /IM "iyw-claw-mcp*.exe" /FI "STATUS eq RUNNING"'
  Pop $0
  Sleep 500

  ${If} $UpdateMode = 1
    Goto iyw_uninstall_done
  ${EndIf}

  ReadRegStr $IywClawRoot SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "InstallRoot"
  StrCmp $IywClawRoot "" iyw_uninstall_done 0
  GetFullPathName $IywClawRoot "$IywClawRoot"

  ; “彻底删除”是独立确认动作（应用卸载对话框传入 /PURGE）：移除全部受管内容
  ; 与用户数据。默认卸载保留用户数据（config/data/skills/user），仅清理可重建
  ; 的受管运行时与日志。
  ${If} $CmdLine == *"/PURGE"*
    GetFullPathName $R8 "$IywClawRoot"
    StrCmp $R8 "" iyw_uninstall_done 0
    DetailPrint "彻底删除模式：正在移除全部安装内容..."
    RMDir /r "$R8"
    DeleteRegKey SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}"
    Goto iyw_uninstall_done
  ${EndIf}

  GetFullPathName $R8 "$IywClawRoot\app"
  GetFullPathName $R9 "$INSTDIR"
  StrCmp $R8 $R9 iyw_remove_managed_dirs 0
  GetFullPathName $R7 "$IywClawRoot"
  StrCmp $R9 $R7 iyw_uninstall_from_root iyw_uninstall_done

  iyw_uninstall_from_root:
    StrCpy $INSTDIR "$IywClawRoot\app"
    SetOutPath "$INSTDIR"

  iyw_remove_managed_dirs:
    DetailPrint "正在删除可重建的受管运行环境..."
    RMDir /r "$IywClawRoot\runtime"
    RMDir /r "$IywClawRoot\staging"
    RMDir /r "$IywClawRoot\logs"
    DeleteRegKey SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}"
    DetailPrint "用户配置、本地数据、Skill 与受管库存已保留。"

  iyw_uninstall_done:
!macroend
