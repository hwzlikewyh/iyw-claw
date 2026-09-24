Var IywClawStatusFont
Var IywClawHeadingFont
Var IywClawPercentFont
Var IywClawBrandFont
Var IywClawProgressHeading
Var IywClawProgressStage
!include "${__FILEDIR__}\installer-steps.nsh"

!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "${__FILEDIR__}\installer-header.bmp"

Function IywClawStyleInitialization
  FindWindow $R0 "#32770" "" $HWNDPARENT
  SetCtlColors $HWNDPARENT 20242C FFFFFF
  SetCtlColors $R0 20242C FFFFFF
  Call IywClawArrangeInitialization
  Call IywClawStyleFonts
  Call IywClawCreateSteps
  Call IywClawStyleHeader
  SetCtlColors $IywClawProgressText 137E87 FFFFFF
  SetCtlColors $IywClawProgressStage 525966 FFFFFF
  GetDlgItem $R1 $R0 1027
  ShowWindow $R1 0
  GetDlgItem $R1 $R0 1016
  ShowWindow $R1 0
  GetDlgItem $R1 $HWNDPARENT 1028
  ShowWindow $R1 0
  GetDlgItem $R1 $HWNDPARENT 1256
  ShowWindow $R1 0
  System::Call 'uxtheme::SetWindowTheme(p $IywClawProgressBar, w "", w "")'
  SendMessage $IywClawProgressBar 0x409 0 0x00877E13
  SendMessage $IywClawProgressBar 0x2001 0 0x00F0EEEB
  SendMessage $IywClawProgressText ${WM_SETTEXT} 0 "STR:0%"
FunctionEnd

Function IywClawStyleFonts
  CreateFont $IywClawHeadingFont "Microsoft YaHei UI" 17 600
  CreateFont $IywClawStatusFont "Microsoft YaHei UI" 9 400
  CreateFont $IywClawPercentFont "Segoe UI" 24 600
  CreateFont $IywClawBrandFont "Microsoft YaHei UI" 14 600
  SendMessage $IywClawProgressHeading ${WM_SETFONT} $IywClawHeadingFont 1
  SendMessage $IywClawProgressStage ${WM_SETFONT} $IywClawStatusFont 1
  SendMessage $IywClawProgressText ${WM_SETFONT} $IywClawPercentFont 1
FunctionEnd

Function IywClawStyleHeader
  InitPluginsDir
  File /oname=$PLUGINSDIR\iyw-installer.ico "${__FILEDIR__}\..\icons\icon.ico"
  GetDlgItem $R1 $HWNDPARENT 1046
  ShowWindow $R1 0
  System::Call 'kernel32::MulDiv(i 48, i R2, i 96) i.R3'
  System::Call 'kernel32::MulDiv(i 10, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 32, i R2, i 96) i.R6'
  System::Call 'user32::LoadImageW(p 0, w "$PLUGINSDIR\iyw-installer.ico", i 1, i R6, i R6, i 0x10) p.R7'
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "", i 0x50000003, i R3, i R5, i R6, i R6, p $HWNDPARENT, p 0, p 0, p 0) p.R1'
  SendMessage $R1 0x170 $R7 0
  GetDlgItem $R1 $HWNDPARENT 1037
  ShowWindow $R1 0
  System::Call 'kernel32::MulDiv(i 92, i R2, i 96) i.R3'
  System::Call 'kernel32::MulDiv(i 12, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 220, i R2, i 96) i.R4'
  System::Call 'kernel32::MulDiv(i 28, i R2, i 96) i.R6'
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "原助理", i 0x50000000, i R3, i R5, i R4, i R6, p $HWNDPARENT, p 2451, p 0, p 0) p.R1'
  SetCtlColors $R1 20242C FFFFFF
  SendMessage $R1 ${WM_SETTEXT} 0 "STR:原助理"
  SendMessage $R1 ${WM_SETFONT} $IywClawBrandFont 1
  GetDlgItem $R1 $HWNDPARENT 1038
  ShowWindow $R1 0
  System::Call 'kernel32::MulDiv(i 370, i R2, i 96) i.R3'
  System::Call 'kernel32::MulDiv(i 21, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 90, i R2, i 96) i.R4'
  System::Call 'kernel32::MulDiv(i 20, i R2, i 96) i.R6'
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "安装向导", i 0x50000000, i R3, i R5, i R4, i R6, p $HWNDPARENT, p 2452, p 0, p 0) p.R1'
  SetCtlColors $R1 747B86 FFFFFF
  SendMessage $R1 ${WM_SETTEXT} 0 "STR:安装向导"
  SendMessage $R1 ${WM_SETFONT} $IywClawStatusFont 1
FunctionEnd

Function IywClawArrangeInitialization
  System::Alloc 16
  Pop $R1
  System::Call 'user32::GetClientRect(p R0, p R1)'
  System::Call '*$R1(i, i, i.R4, i.R5)'
  System::Free $R1
  System::Call 'user32::GetDpiForWindow(p R0) i.R2'
  System::Call 'kernel32::MulDiv(i 24, i R2, i 96) i.R3'
  IntOp $R4 $R4 - $R3
  IntOp $R4 $R4 - $R3
  System::Call 'kernel32::MulDiv(i 74, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 32, i R2, i 96) i.R6'
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "正在初始化", i 0x50000000, i R3, i R5, i R4, i R6, p R0, p 0, p 0, p 0) p.R1'
  StrCpy $IywClawProgressHeading $R1
  SetCtlColors $IywClawProgressHeading 20242C FFFFFF
  System::Call 'kernel32::MulDiv(i 115, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 24, i R2, i 96) i.R6'
  System::Call 'kernel32::MulDiv(i 100, i R2, i 96) i.R7'
  IntOp $R7 $R4 - $R7
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "1 / 3   准备安装", i 0x50000000, i R3, i R5, i R7, i R6, p R0, p 0, p 0, p 0) p.R1'
  StrCpy $IywClawProgressStage $R1
  System::Call 'kernel32::MulDiv(i 159, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 8, i R2, i 96) i.R6'
  System::Call 'user32::SetWindowPos(p $IywClawProgressBar, p 0, i R3, i R5, i R4, i R6, i 0x40)'
  System::Call 'kernel32::MulDiv(i 96, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 44, i R2, i 96) i.R6'
  System::Call 'kernel32::MulDiv(i 96, i R2, i 96) i.R8'
  IntOp $R9 $R3 + $R4
  IntOp $R9 $R9 - $R8
  System::Call 'user32::SetWindowPos(p $IywClawProgressText, p 0, i R9, i R5, i R8, i R6, i 0x40)'
  System::Call 'user32::BringWindowToTop(p $IywClawProgressHeading)'
  System::Call 'user32::BringWindowToTop(p $IywClawProgressStage)'
  System::Call 'user32::BringWindowToTop(p $IywClawProgressBar)'
  System::Call 'user32::BringWindowToTop(p $IywClawProgressText)'
FunctionEnd
