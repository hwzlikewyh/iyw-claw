Var IywClawOriginalProgress
Var IywClawProgressBar
Var IywClawProgressText
!include "${__FILEDIR__}\installer-appearance.nsh"

; 控件必须由页面 UI 线程创建，安装工作线程没有消息循环。
!ifmacrodef MUI_PAGE_INSTFILES
  !macroundef MUI_PAGE_INSTFILES
  !macro MUI_PAGE_INSTFILES
    !define MUI_PAGE_CUSTOMFUNCTION_SHOW IywClawCreateProgress
    !define MUI_INSTFILESPAGE_FINISHHEADER_TEXT "初始化完成"
    !define MUI_INSTFILESPAGE_FINISHHEADER_SUBTEXT "原助理已准备就绪。"
    !insertmacro MUI_PAGE_INIT
    !insertmacro MUI_PAGEDECLARATION_INSTFILES
  !macroend
!endif

Function IywClawCreateProgress
  SetDetailsPrint none
  FindWindow $R0 "#32770" "" $HWNDPARENT
  GetDlgItem $IywClawOriginalProgress $R0 1004
  GetDlgItem $R1 $R0 1006
  ShowWindow $R1 0
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "0%", i 0x50000002, i 0, i 0, i 0, i 0, p R0, p 0, p 0, p 0) p.R1'
  StrCpy $IywClawProgressText $R1
  StrCmp $IywClawOriginalProgress "0" progress_created 0
  System::Alloc 16
  Pop $R1
  System::Call 'user32::GetWindowRect(p $IywClawOriginalProgress, p R1)'
  System::Call 'user32::MapWindowPoints(p 0, p R0, p R1, i 2)'
  System::Call '*$R1(i.R2, i.R3, i.R4, i.R5)'
  System::Free $R1
  IntOp $R4 $R4 - $R2
  IntOp $R5 $R5 - $R3
  System::Call 'user32::CreateWindowExW(i 0, w "msctls_progress32", w "", i 0x50000001, i R2, i R3, i R4, i R5, p R0, p 0, p 0, p 0) p.R1'
  StrCpy $IywClawProgressBar $R1
  StrCmp $IywClawProgressBar "0" progress_created 0
  ShowWindow $IywClawOriginalProgress 0
  SendMessage $IywClawProgressBar 0x406 0 10000
  SendMessage $IywClawProgressText ${WM_SETTEXT} 0 "STR:正在初始化..."
  Call IywClawStyleInitialization
  progress_created:
FunctionEnd

Function IywClawFinishProgress
  StrCmp $IywClawProgressBar "" progress_finished 0
  SendMessage $IywClawProgressBar 0x402 10000 0
  SendMessage $IywClawProgressHeading ${WM_SETTEXT} 0 "STR:初始化完成"
  SendMessage $IywClawProgressText ${WM_SETTEXT} 0 "STR:100%"
  progress_finished:
FunctionEnd
