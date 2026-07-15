; Capture source paths while NSIS includes this file.
!define IYW_CLAW_INSTALL_MANAGED_NODE_SCRIPT "${__FILEDIR__}\..\..\install-managed-node.ps1"
!define IYW_CLAW_PREPARE_MANAGED_NODE_SCRIPT "${__FILEDIR__}\..\..\prepare-managed-node.ps1"
!define IYW_CLAW_INSTALL_MANAGED_GIT_SCRIPT "${__FILEDIR__}\..\..\install-managed-git.ps1"
!define IYW_CLAW_PREPARE_MANAGED_GIT_SCRIPT "${__FILEDIR__}\..\..\prepare-managed-git.ps1"
!define IYW_CLAW_MANAGED_NODE_CACHE_DIR "${__FILEDIR__}\..\target\managed-node"
!define IYW_CLAW_MANAGED_GIT_CACHE_DIR "${__FILEDIR__}\..\target\managed-git"

!define IYW_CLAW_INSTALL_REGISTRY_KEY "Software\iywclaw\iyw-claw"
!define IYW_CLAW_NODE_VERSION "24.0.0"
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
  nsExec::Exec 'taskkill /F /T /IM iyw-claw-mcp.exe'
  Pop $0
  Sleep 500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Tauri persists the internal app directory as the next installer location.
  ; Expose the logical root in the directory page while keeping binaries
  ; isolated below root\app.
  WriteRegStr SHCTX "${IYW_CLAW_INSTALL_REGISTRY_KEY}" "" "$IywClawRoot"

  ; ARCH is defined by Tauri's generated installer.nsi after this hook is
  ; included, so resolve architecture-specific assets when this macro expands.
  !if "${ARCH}" == "x64"
    !define IYW_CLAW_MANAGED_NODE_ASSET "node-v24.0.0-win-x64.zip"
    !define IYW_CLAW_MANAGED_GIT_ASSET "MinGit-2.55.0.2-64-bit.zip"
  !else if "${ARCH}" == "arm64"
    !define IYW_CLAW_MANAGED_NODE_ASSET "node-v24.0.0-win-arm64.zip"
    !define IYW_CLAW_MANAGED_GIT_ASSET "MinGit-2.55.0.2-arm64.zip"
  !else
    !error "Unsupported Windows installer architecture: ${ARCH}"
  !endif

  DetailPrint "正在准备内置 Node.js/npm 运行环境..."
  !system 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${IYW_CLAW_PREPARE_MANAGED_NODE_SCRIPT}" -Architecture "${ARCH}" -Version "${IYW_CLAW_NODE_VERSION}" -OutputDirectory "${IYW_CLAW_MANAGED_NODE_CACHE_DIR}"' = 0
  File /oname=$PLUGINSDIR\managed-node.zip "${IYW_CLAW_MANAGED_NODE_CACHE_DIR}\${IYW_CLAW_MANAGED_NODE_ASSET}"
  File /oname=$PLUGINSDIR\node-shasums.txt "${IYW_CLAW_MANAGED_NODE_CACHE_DIR}\SHASUMS256.txt"
  File /oname=$PLUGINSDIR\install-managed-node.ps1 "${IYW_CLAW_INSTALL_MANAGED_NODE_SCRIPT}"

  iyw_install_node:
    nsExec::ExecToLog 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\install-managed-node.ps1" -Version "${IYW_CLAW_NODE_VERSION}" -RuntimeRoot "$IywClawRoot\runtime" -ArchivePath "$PLUGINSDIR\managed-node.zip" -ChecksumPath "$PLUGINSDIR\node-shasums.txt" -LogPath "$IywClawRoot\logs\installer.log"'
    Pop $0
    StrCmp $0 0 iyw_node_done 0
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
      "Node.js/npm 运行环境安装失败，错误码：$0。$\r$\n$\r$\n请选择重试或取消安装。" \
      IDRETRY iyw_install_node
    Abort

  iyw_node_done:
    DetailPrint "正在准备内置 Git 运行环境..."
    !system 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${IYW_CLAW_PREPARE_MANAGED_GIT_SCRIPT}" -Architecture "${ARCH}" -OutputDirectory "${IYW_CLAW_MANAGED_GIT_CACHE_DIR}"' = 0
    File /oname=$PLUGINSDIR\managed-git.zip "${IYW_CLAW_MANAGED_GIT_CACHE_DIR}\${IYW_CLAW_MANAGED_GIT_ASSET}"
    File /oname=$PLUGINSDIR\install-managed-git.ps1 "${IYW_CLAW_INSTALL_MANAGED_GIT_SCRIPT}"

  iyw_install_git:
    nsExec::ExecToLog 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\install-managed-git.ps1" -RuntimeRoot "$IywClawRoot\runtime" -ArchivePath "$PLUGINSDIR\managed-git.zip" -LogPath "$IywClawRoot\logs\installer.log"'
    Pop $0
    StrCmp $0 0 iyw_git_done 0
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
      "Git 运行环境安装失败，错误码：$0。$\r$\n$\r$\n请选择重试或取消安装。" \
      IDRETRY iyw_install_git
    Abort

  iyw_git_done:
    DetailPrint "内核运行环境准备完成。"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "正在停止运行中的 iyw-claw 后台进程..."
  nsExec::Exec 'taskkill /F /T /IM iyw-claw-mcp.exe'
  Pop $0
  Sleep 500

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
