; Like FilmCraft – közös telepítő (FilmCraft + LIKE Diktálás beszédmotor).
;
; Felhasználói telepítés (nem kell rendszergazda): %LOCALAPPDATA%\Like FilmCraft
; Asztali és Start menü ikon, eltávolító (Programok és szolgáltatások).
; A LIKE Diktálás telepítőjét (ha a fordításkor megadták) csendes módban futtatja; a FilmCraft
; a motort a %LOCALAPPDATA%\LIKE Diktálás\ mappában magától megtalálja.
;
; Fordítás: packaging/like/build-installer.ps1 (ez adja meg a változókat).
;   BINDIR      – a filmcraft.exe és filmcraft-cli.exe mappája (target\release)
;   ROOTDIR     – a tároló gyökere (NOTICE, LICENSE-*, ikon)
;   VERSION     – megjelenített verzió, pl. 0.17.0-like.10
;   OUTFILE     – a kész telepítő útvonala
;   DIKTALAS    – (nem kötelező) a LIKE Diktálás telepítő .exe útvonala

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user
ManifestDPIAware true

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!define APPNAME "Like FilmCraft"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Like FilmCraft"

Name "${APPNAME}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Like FilmCraft"
InstallDirRegKey HKCU "${UNINSTKEY}" "InstallLocation"
BrandingText "${APPNAME} ${VERSION}"

!define MUI_ICON "${ROOTDIR}\assets\app-icon\filmcraft.ico"
!define MUI_UNICON "${ROOTDIR}\assets\app-icon\filmcraft.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\filmcraft.exe"
!define MUI_FINISHPAGE_RUN_TEXT "A Like FilmCraft indítása"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Hungarian"

VIProductVersion "0.17.0.0"
VIAddVersionKey /LANG=${LANG_HUNGARIAN} "ProductName" "${APPNAME}"
VIAddVersionKey /LANG=${LANG_HUNGARIAN} "FileDescription" "${APPNAME} telepítő"
VIAddVersionKey /LANG=${LANG_HUNGARIAN} "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_HUNGARIAN} "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_HUNGARIAN} "LegalCopyright" "MIT / Apache-2.0"

Section "Like FilmCraft (videószerkesztő)" SecApp
  SectionIn RO
  SetOutPath "$INSTDIR"
  ; Ha a program fut, a fájl nem írható felül: az NSIS ilyenkor Újra/Mégse ablakot mutat.
  SetOverwrite on
  File "${BINDIR}\filmcraft.exe"
  File "${BINDIR}\filmcraft-cli.exe"
  File "${ROOTDIR}\NOTICE"
  File "${ROOTDIR}\LICENSE-MIT"
  File "${ROOTDIR}\LICENSE-APACHE"
  File "${ROOTDIR}\docs\LIKE-HASZNALATI-UTMUTATO.md"

  CreateShortcut "$DESKTOP\${APPNAME}.lnk" "$INSTDIR\filmcraft.exe" "" "$INSTDIR\filmcraft.exe" 0
  CreateDirectory "$SMPROGRAMS\${APPNAME}"
  CreateShortcut "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk" "$INSTDIR\filmcraft.exe" "" "$INSTDIR\filmcraft.exe" 0
  CreateShortcut "$SMPROGRAMS\${APPNAME}\Használati útmutató.lnk" "$INSTDIR\LIKE-HASZNALATI-UTMUTATO.md"
  CreateShortcut "$SMPROGRAMS\${APPNAME}\${APPNAME} eltávolítása.lnk" "$INSTDIR\uninstall.exe"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "Like FilmCraft (FilmCraft fork)"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" "$INSTDIR\filmcraft.exe,0"
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINSTKEY}" "EstimatedSize" $0
SectionEnd

!ifdef DIKTALAS
Section "LIKE Diktálás beszédmotor (magyar leirat, felirat, AI)" SecEngine
  SetOutPath "$PLUGINSDIR"
  File "/oname=LIKE-Diktalas-Telepito.exe" "${DIKTALAS}"
  DetailPrint "A LIKE Diktálás telepítése…"
  ExecWait '"$PLUGINSDIR\LIKE-Diktalas-Telepito.exe" /S' $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "A LIKE Diktálás telepítése nem sikerült (kód: $0). A FilmCraft működik, de a LIKE menü magyar leirat- és feliratfunkciói a Diktálás nélkül szürkék maradnak."
  ${EndIf}
  Delete "$PLUGINSDIR\LIKE-Diktalas-Telepito.exe"
SectionEnd
!endif

; /NODIKTALAS a parancssorban: csak a FilmCraft települ (a Diktálás már a legfrissebb, vagy éppen fut).
Function .onInit
!ifdef DIKTALAS
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/NODIKTALAS" $R1
  ${IfNot} ${Errors}
    SectionSetFlags ${SecEngine} 0
  ${EndIf}
!endif
FunctionEnd

LangString DESC_App ${LANG_HUNGARIAN} "A videószerkesztő program magyar felülettel és a LIKE menüvel."
!ifdef DIKTALAS
LangString DESC_Engine ${LANG_HUNGARIAN} "A beszédfelismerő motor a magyar leirathoz, felirathoz és az AI-javításhoz. Ha már telepítve van, frissül; a beállításaid megmaradnak."
!endif
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} $(DESC_App)
!ifdef DIKTALAS
  !insertmacro MUI_DESCRIPTION_TEXT ${SecEngine} $(DESC_Engine)
!endif
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  ; Csak a saját fájljainkat töröljük; a projektek és a beállítások megmaradnak.
  ; A LIKE Diktálás külön program, a saját eltávolítójával távolítható el.
  Delete "$INSTDIR\filmcraft.exe"
  Delete "$INSTDIR\filmcraft-cli.exe"
  Delete "$INSTDIR\NOTICE"
  Delete "$INSTDIR\LICENSE-MIT"
  Delete "$INSTDIR\LICENSE-APACHE"
  Delete "$INSTDIR\LIKE-HASZNALATI-UTMUTATO.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$DESKTOP\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\${APPNAME}\${APPNAME}.lnk"
  Delete "$SMPROGRAMS\${APPNAME}\Használati útmutató.lnk"
  Delete "$SMPROGRAMS\${APPNAME}\${APPNAME} eltávolítása.lnk"
  RMDir "$SMPROGRAMS\${APPNAME}"
  DeleteRegKey HKCU "${UNINSTKEY}"
SectionEnd
