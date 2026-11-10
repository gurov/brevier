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
!include "WinMessages.nsh"

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

; Шрифты окна — в систему, для одного читателя (Windows 10 1809 и новее
; читает их из HKCU): иначе окно отдаёт их Pango на каждом запуске, а это
; секунды на машине с сотнями шрифтов. Имя записи — со словом Brevier:
; Noto Sans, поставленный читателем самим, мы не перезаписываем и при
; удалении не трогаем. На старой Windows запись ничего не даст — окно
; увидит, что шрифтов нет, и отдаст их само.
!define FONTS_KEY "Software\Microsoft\Windows NT\CurrentVersion\Fonts"
!macro Font file name
    File "${FONTS}\${file}"
    WriteRegStr HKCU "${FONTS_KEY}" "${name} (Brevier)" "$INSTDIR\fonts\${file}"
    System::Call 'gdi32::AddFontResourceW(w "$INSTDIR\fonts\${file}") i'
!macroend
!macro UnFont file name
    System::Call 'gdi32::RemoveFontResourceW(w "$INSTDIR\fonts\${file}") i'
    DeleteRegValue HKCU "${FONTS_KEY}" "${name} (Brevier)"
    Delete /REBOOTOK "$INSTDIR\fonts\${file}"
!macroend
!macro Fonts how
    !insertmacro ${how} "NotoSans-Light.ttf" "Noto Sans Light"
    !insertmacro ${how} "NotoSans-Regular.ttf" "Noto Sans"
    !insertmacro ${how} "NotoSans-Italic.ttf" "Noto Sans Italic"
    !insertmacro ${how} "NotoSans-Medium.ttf" "Noto Sans Medium"
    !insertmacro ${how} "NotoSans-Bold.ttf" "Noto Sans Bold"
    !insertmacro ${how} "NotoSans-BoldItalic.ttf" "Noto Sans Bold Italic"
    !insertmacro ${how} "NotoSansMono-Regular.ttf" "Noto Sans Mono"
!macroend

; Открытый Brevier держит свои файлы: установщик стёр бы половину `bin`
; и споткнулся на занятых, а `winget upgrade` ставит поверх молча. Запущенный
; exe на запись не открыть — так и смотрим, пока ничего не стёрто. gdbus.exe
; уходит сам через секунды после окна, поэтому сначала подождём. Без окна
; (`/S`) вопрос отвечает себе «отмена»: выходим, ничего не тронув, с кодом
; STILL_OPEN — манифест winget называет его packageInUse.
!define STILL_OPEN 10
!macro Taken file
    IfFileExists "$INSTDIR\bin\${file}" 0 +3
    FileOpen $0 "$INSTDIR\bin\${file}" a
    FileClose $0
!macroend
!macro UntilClosed un
    Function ${un}UntilClosed
        StrCpy $1 0
        look:
        ClearErrors
        !insertmacro Taken "brevier-ui.exe"
        !insertmacro Taken "brevier.exe"
        !insertmacro Taken "gdbus.exe"
        IfErrors 0 closed
        IntOp $1 $1 + 1
        Sleep 1000
        IntCmp $1 5 0 look
        StrCpy $1 0
        MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
            "Brevier is open. Close it, then press Retry." /SD IDCANCEL IDRETRY look
        SetErrorLevel ${STILL_OPEN}
        Quit
        closed:
    FunctionEnd
!macroend
!insertmacro UntilClosed ""
!insertmacro UntilClosed "un."

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
    Call UntilClosed

    ; Новая версия ложится на чистое место: библиотека от прошлой, которой
    ; в новой нет, не должна подхватиться окном.
    RMDir /r "$INSTDIR\bin"
    RMDir /r "$INSTDIR\lib"
    RMDir /r "$INSTDIR\share"
    RMDir /r "$INSTDIR\licenses"

    SetOutPath "$INSTDIR"
    File /r "${STAGE}\*"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateShortcut "$SMPROGRAMS\Brevier.lnk" "${EXE}"

    ; Папку шрифтов не стираем, как прочие: шрифт, открытый другой
    ; программой, не перезаписать. Не вышло — остаётся прежний файл:
    ; гарнитуры меняются раз в годы.
    SetOutPath "$INSTDIR\fonts"
    SetOverwrite try
    !insertmacro Fonts Font
    SetOverwrite on
    SendMessage ${HWND_BROADCAST} ${WM_FONTCHANGE} 0 0 /TIMEOUT=5000

    ; Строка в «Установленных приложениях».
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Brevier"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Pavel Gurov"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "${EXE},0"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/gurov/brevier"
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $0
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1

    ; Brevier — кандидат в браузеры и в «Открыть с помощью» для markdown,
    ; лент и сохранённых архивов `.zip`, как его ярлык на Linux. Выбор
    ; по умолчанию остаётся за читателем: Windows не даёт программе
    ; назначить себя самой.
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
    !insertmacro Opens ".zip"
    WriteRegStr HKCU "Software\RegisteredApplications" "Brevier" "${CLIENT_KEY}\Capabilities"

    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

Section "Uninstall"
    Call un.UntilClosed

    Delete "$SMPROGRAMS\Brevier.lnk"
    !insertmacro Fonts UnFont
    RMDir "$INSTDIR\fonts"
    SendMessage ${HWND_BROADCAST} ${WM_FONTCHANGE} 0 0 /TIMEOUT=5000
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
    !insertmacro UnOpens ".zip"

    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd
