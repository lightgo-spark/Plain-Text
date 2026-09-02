; Anti-library — a per-user installer.
;
; Per-user on purpose. A machine-wide install needs administrator rights, and
; asking for them is asking a reader to trust an unsigned program with the
; whole machine to read a text file. Everything here lands under the user's own
; profile, the uninstall entry is theirs, and nothing touches HKLM.
;
; Built by dist.ps1, which passes VERSION, STAGE and OUTFILE.

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef STAGE
  !error "STAGE (the folder holding the built files) must be defined"
!endif
!ifndef OUTFILE
  !define OUTFILE "anti-library-setup.exe"
!endif
; Absolute, because makensis runs with /NOCD: a relative path here is
; resolved against whatever directory the build started in.
!ifndef ROOT
  !error "ROOT (the project directory) must be defined"
!endif

!define APPNAME    "Anti-library"
!define PUBLISHER  "Anti-library"
!define REGKEY     "Software\Microsoft\Windows\CurrentVersion\Uninstall\Anti-library"

Unicode true
Name "${APPNAME} ${VERSION}"
OutFile "${OUTFILE}"
; Per-user: no elevation prompt, and no chance of an unsigned installer being
; handed the whole machine.
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Anti-library"
InstallDirRegKey HKCU "Software\Anti-library" "InstallDir"
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUnInstDetails show

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "WordFunc.nsh"
!include "WinMessages.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "${ROOT}\assets\icon.ico"
!define MUI_UNICON "${ROOT}\assets\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\antilib-gui.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Anti-library"

!insertmacro MUI_PAGE_LICENSE "${ROOT}\LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_COMPONENTS
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Anti-library (required)" SecCore
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${STAGE}\antilib-gui.exe"
  File "${STAGE}\antilib.exe"
  File "${STAGE}\antilib-bench.exe"
  File "${STAGE}\LICENSE"
  File "${STAGE}\NOTICES.md"
  File "${STAGE}\THIRD-PARTY-LICENSES.md"
  File "${STAGE}\README.md"

  WriteRegStr HKCU "Software\Anti-library" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Anti-library" "Version" "${VERSION}"

  ; The entry in Settings > Apps. Without it the only way to remove this is to
  ; delete a folder and wonder what else was left behind.
  WriteRegStr HKCU "${REGKEY}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "${REGKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${REGKEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${REGKEY}" "DisplayIcon" "$INSTDIR\antilib-gui.exe"
  WriteRegStr HKCU "${REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${REGKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${REGKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${REGKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${REGKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${REGKEY}" "EstimatedSize" "$0"

  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Start menu shortcut" SecStartMenu
  CreateDirectory "$SMPROGRAMS\Anti-library"
  CreateShortCut "$SMPROGRAMS\Anti-library\Anti-library.lnk" "$INSTDIR\antilib-gui.exe"
  CreateShortCut "$SMPROGRAMS\Anti-library\Uninstall Anti-library.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

; Off by default. Taking over a file type is the installer deciding something
; about a machine it was only asked to put a program on — and .txt in
; particular already belongs to something the reader chose.
Section /o "Open .epub files with Anti-library" SecEpub
  WriteRegStr HKCU "Software\Classes\Anti-library.Book" "" "Book"
  WriteRegStr HKCU "Software\Classes\Anti-library.Book\DefaultIcon" "" "$INSTDIR\antilib-gui.exe,0"
  WriteRegStr HKCU "Software\Classes\Anti-library.Book\shell\open\command" "" '"$INSTDIR\antilib-gui.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\.epub\OpenWithProgids" "Anti-library.Book" ""
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
SectionEnd

Section /o "Add to PATH (for the terminal reader)" SecPath
  ; HKCU only, and appended — a PATH edit that replaces is a PATH edit that
  ; breaks something else.
  ReadRegStr $0 HKCU "Environment" "Path"
  ${WordFind} "$0" "$INSTDIR" "E+1{" $1
  IfErrors 0 pathAlreadyThere
    StrCmp $0 "" 0 +3
      WriteRegExpandStr HKCU "Environment" "Path" "$INSTDIR"
      Goto pathDone
    WriteRegExpandStr HKCU "Environment" "Path" "$0;$INSTDIR"
  pathDone:
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=2000
  pathAlreadyThere:
SectionEnd

LangString DESC_SecCore ${LANG_ENGLISH} "The reader itself: the desktop reader, the terminal reader, and the benchmark."
LangString DESC_SecStartMenu ${LANG_ENGLISH} "A shortcut in the Start menu."
LangString DESC_SecEpub ${LANG_ENGLISH} "Adds Anti-library to the 'Open with' list for .epub files. It does not become the default."
LangString DESC_SecPath ${LANG_ENGLISH} "Lets you run 'antilib' from a terminal anywhere."

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCore} $(DESC_SecCore)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecStartMenu} $(DESC_SecStartMenu)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecEpub} $(DESC_SecEpub)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecPath} $(DESC_SecPath)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; ---------------------------------------------------------------------------

Section "un.Anti-library" UnCore
  SectionIn RO
  Delete "$INSTDIR\antilib-gui.exe"
  Delete "$INSTDIR\antilib.exe"
  Delete "$INSTDIR\antilib-bench.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\NOTICES.md"
  Delete "$INSTDIR\THIRD-PARTY-LICENSES.md"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\Anti-library\Anti-library.lnk"
  Delete "$SMPROGRAMS\Anti-library\Uninstall Anti-library.lnk"
  RMDir "$SMPROGRAMS\Anti-library"

  DeleteRegKey HKCU "Software\Classes\Anti-library.Book"
  DeleteRegValue HKCU "Software\Classes\.epub\OpenWithProgids" "Anti-library.Book"
  DeleteRegKey HKCU "${REGKEY}"
  DeleteRegKey HKCU "Software\Anti-library"
SectionEnd

; Unticked, and last. Bookmarks, highlights and notes are the reader's own
; work and the only thing here that cannot be downloaded again; an uninstaller
; that takes them by default is one that eats somebody's marginalia because
; they wanted the disk space back.
Section /o "un.Also delete my bookmarks, highlights and notes" UnData
  Delete "$APPDATA\anti-library\library.json"
  Delete "$APPDATA\anti-library\reader.json"
  Delete "$APPDATA\anti-library\crash.log"
  RMDir "$APPDATA\anti-library"
SectionEnd

LangString DESC_UnCore ${LANG_ENGLISH} "Removes the program."
LangString DESC_UnData ${LANG_ENGLISH} "Also removes your reading positions, bookmarks, highlights and notes. Leave this unticked to keep them for a later install."

!insertmacro MUI_UNFUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${UnCore} $(DESC_UnCore)
  !insertmacro MUI_DESCRIPTION_TEXT ${UnData} $(DESC_UnData)
!insertmacro MUI_UNFUNCTION_DESCRIPTION_END
