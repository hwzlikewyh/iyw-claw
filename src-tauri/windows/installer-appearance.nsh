Var IywClawStatusFont
Var IywClawHeadingFont
Var IywClawProgressHeading

!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "${__FILEDIR__}\installer-header.bmp"

Function IywClawStyleInitialization
  FindWindow $R0 "#32770" "" $HWNDPARENT
  SetCtlColors $R0 17212B FFFFFF
  Call IywClawArrangeInitialization
  SetCtlColors $IywClawProgressText 374151 FFFFFF
  GetDlgItem $R1 $R0 1027
  ShowWindow $R1 0
  GetDlgItem $R1 $R0 1016
  ShowWindow $R1 0
  System::Call 'user32::GetDpiForWindow(p $R0) i.R2'
  System::Call 'kernel32::MulDiv(i -14, i R2, i 96) i.R3'
  System::Call 'gdi32::CreateFontW(i R3, i 0, i 0, i 0, i 400, i 0, i 0, i 0, i 1, i 0, i 0, i 5, i 0, w "Microsoft YaHei UI") p.R1'
  StrCpy $IywClawStatusFont $R1
  SendMessage $IywClawProgressText ${WM_SETFONT} $IywClawStatusFont 1
  System::Call 'kernel32::MulDiv(i -22, i R2, i 96) i.R3'
  System::Call 'gdi32::CreateFontW(i R3, i 0, i 0, i 0, i 600, i 0, i 0, i 0, i 1, i 0, i 0, i 5, i 0, w "Microsoft YaHei UI") p.R1'
  StrCpy $IywClawHeadingFont $R1
  SendMessage $IywClawProgressHeading ${WM_SETFONT} $IywClawHeadingFont 1
  GetDlgItem $R1 $HWNDPARENT 1037
  SendMessage $R1 ${WM_SETTEXT} 0 "STR:原助理"
  GetDlgItem $R1 $HWNDPARENT 1038
  SendMessage $R1 ${WM_SETTEXT} 0 "STR:安装程序"
  System::Call 'uxtheme::SetWindowTheme(p $IywClawProgressBar, w "", w "")'
  SendMessage $IywClawProgressBar 0x409 0 0x009A8F10
  SendMessage $IywClawProgressBar 0x2001 0 0x00EBE7E5
  SendMessage $IywClawProgressText ${WM_SETTEXT} 0 "STR:0%"
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
  System::Call 'kernel32::MulDiv(i 42, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 32, i R2, i 96) i.R6'
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "正在初始化", i 0x50000000, i R3, i R5, i R4, i R6, p R0, p 0, p 0, p 0) p.R1'
  StrCpy $IywClawProgressHeading $R1
  SetCtlColors $IywClawProgressHeading 17212B FFFFFF
  System::Call 'kernel32::MulDiv(i 86, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 10, i R2, i 96) i.R6'
  System::Call 'user32::SetWindowPos(p $IywClawProgressBar, p 0, i R3, i R5, i R4, i R6, i 0x40)'
  System::Call 'kernel32::MulDiv(i 112, i R2, i 96) i.R5'
  System::Call 'kernel32::MulDiv(i 24, i R2, i 96) i.R6'
  System::Call 'user32::SetWindowPos(p $IywClawProgressText, p 0, i R3, i R5, i R4, i R6, i 0x40)'
  System::Call 'user32::BringWindowToTop(p $IywClawProgressHeading)'
  System::Call 'user32::BringWindowToTop(p $IywClawProgressBar)'
  System::Call 'user32::BringWindowToTop(p $IywClawProgressText)'
FunctionEnd
