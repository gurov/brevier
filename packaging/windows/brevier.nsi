; Установщик Brevier для Windows. Собирает его packaging/windows/build.sh:
;
;     makensis -DVERSION=X.Y.Z -DSTAGE=<папка программы> -DOUTFILE=<setup.exe> brevier.nsi
;
; Ставит для одного пользователя и без прав администратора — в
; %LOCALAPPDATA%\Programs\Brevier, как ставятся «для себя» VS Code и Chrome:
; окно UAC на программу для чтения было бы лишним вопросом. Новая версия
; ставится поверх старой тем же установщиком; данные читателя
; (%LOCALAPPDATA%\Brevier) удаление не трогает, как и у браузеров.

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user
ManifestDPIAware true

!include "MUI2.nsh"
!include "FileFunc.nsh"

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Brevier"
!define CLIENT_KEY "Software\Clients\StartMenuInternet\Brevier"
!define URL_CLASS "Brevier.URL"
!define FILE_CLASS "Brevier.Document"
!define EXE "$INSTDIR\bin\brevier-ui.exe"

; Файлы с этим расширением Brevier открывает: он в «Открыть с помощью»
; и в списке своих возможностей для «Приложений по умолчанию».
!macro Opens extension
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities\FileAssociations" "${extension}" "${FILE_CLASS}"
    WriteRegStr HKCU "Software\Classes\${extension}\OpenWithProgids" "${FILE_CLASS}" ""
!macroend
!macro UnOpens extension
    DeleteRegValue HKCU "Software\Classes\${extension}\OpenWithProgids" "${FILE_CLASS}"
!macroend

Name "Brevier"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Brevier"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "Brevier ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Brevier"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "Brevier ${VERSION} setup"
VIAddVersionKey "LegalCopyright" "MIT OR Apache-2.0"

!define MUI_ICON "brevier.ico"
!define MUI_UNICON "brevier.ico"
!define MUI_FINISHPAGE_RUN "${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Start Brevier"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section
    ; Новая версия ложится на чистое место: библиотека от прошлой, которой
    ; в новой нет, не должна подхватиться окном.
    RMDir /r "$INSTDIR\bin"
    RMDir /r "$INSTDIR\lib"
    RMDir /r "$INSTDIR\share"
    RMDir /r "$INSTDIR\licenses"

    SetOutPath "$INSTDIR"
    File /r "${STAGE}/*"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateShortcut "$SMPROGRAMS\Brevier.lnk" "${EXE}"

    ; Строка в «Установленных приложениях».
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Brevier"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "${EXE},0"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/gurov/brevier"
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $0
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1

    ; Brevier — кандидат в браузеры и в «Открыть с помощью» для markdown
    ; и лент, как его ярлык на Linux. Выбор по умолчанию остаётся за
    ; читателем: Windows не даёт программе назначить себя самой.
    WriteRegStr HKCU "Software\Classes\${URL_CLASS}" "" "Brevier URL"
    WriteRegStr HKCU "Software\Classes\${URL_CLASS}\DefaultIcon" "" "${EXE},0"
    WriteRegStr HKCU "Software\Classes\${URL_CLASS}\shell\open\command" "" '"${EXE}" "%1"'
    WriteRegStr HKCU "Software\Classes\${FILE_CLASS}" "" "Document"
    WriteRegStr HKCU "Software\Classes\${FILE_CLASS}\DefaultIcon" "" "${EXE},0"
    WriteRegStr HKCU "Software\Classes\${FILE_CLASS}\shell\open\command" "" '"${EXE}" "%1"'

    WriteRegStr HKCU "${CLIENT_KEY}" "" "Brevier"
    WriteRegStr HKCU "${CLIENT_KEY}\DefaultIcon" "" "${EXE},0"
    WriteRegStr HKCU "${CLIENT_KEY}\shell\open\command" "" '"${EXE}"'
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities" "ApplicationName" "Brevier"
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities" "ApplicationDescription" \
        "Reading without JavaScript, in the typography you chose."
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities" "ApplicationIcon" "${EXE},0"
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities\StartMenu" "StartMenuInternet" "Brevier"
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities\URLAssociations" "http" "${URL_CLASS}"
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities\URLAssociations" "https" "${URL_CLASS}"
    WriteRegStr HKCU "${CLIENT_KEY}\Capabilities\URLAssociations" "feed" "${URL_CLASS}"
    !insertmacro Opens ".md"
    !insertmacro Opens ".markdown"
    !insertmacro Opens ".rss"
    !insertmacro Opens ".atom"
    WriteRegStr HKCU "Software\RegisteredApplications" "Brevier" "${CLIENT_KEY}\Capabilities"

    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

Section "Uninstall"
    Delete "$SMPROGRAMS\Brevier.lnk"
    ; Только своё: папку установки могли выбрать общую, и удалять её
    ; целиком установщик не вправе.
    RMDir /r "$INSTDIR\bin"
    RMDir /r "$INSTDIR\lib"
    RMDir /r "$INSTDIR\share"
    RMDir /r "$INSTDIR\licenses"
    Delete "$INSTDIR\README.txt"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    DeleteRegKey HKCU "${CLIENT_KEY}"
    DeleteRegValue HKCU "Software\RegisteredApplications" "Brevier"
    DeleteRegKey HKCU "Software\Classes\${URL_CLASS}"
    DeleteRegKey HKCU "Software\Classes\${FILE_CLASS}"
    !insertmacro UnOpens ".md"
    !insertmacro UnOpens ".markdown"
    !insertmacro UnOpens ".rss"
    !insertmacro UnOpens ".atom"

    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd
