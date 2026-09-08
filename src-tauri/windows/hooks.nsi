; NSIS installer hooks for Hive Viewer
; Makes Hive Viewer appear in the Windows "Open with" dialog for supported image
; formats. It deliberately does NOT claim the extensions' default ProgId: on
; Windows 8+ the effective default is the per-user UserChoice hash, and an
; installer-written default cannot be restored on uninstall (it used to leave
; `.jpg -> HiveViewer.JPEG` pointing at a deleted ProgId).

!macro RegisterExtension exe ext progId
  ; Advertise the ProgId as an "Open with" candidate only.
  WriteRegStr HKCR ".${ext}\OpenWithProgids" "${progId}" ""

  ; Define the ProgId.
  WriteRegStr HKCR "${progId}" "" "${ext} File"
  WriteRegStr HKCR "${progId}\shell\open\command" "" '"${exe}" "%1"'
  WriteRegStr HKCR "${progId}\DefaultIcon" "" '"${exe}",0'
!macroend

!macro UnregisterExtension ext progId
  ; Clear the extension's default ProgId only while it still points at us, so a
  ; default the user picked later is never clobbered. HKCU covers installs made
  ; before the installer started running elevated; HKLM covers current ones.
  ReadRegStr $0 HKCU "Software\Classes\.${ext}" ""
  ${If} $0 == "${progId}"
    DeleteRegValue HKCU "Software\Classes\.${ext}" ""
  ${EndIf}
  DeleteRegValue HKCU "Software\Classes\.${ext}\OpenWithProgids" "${progId}"

  ReadRegStr $0 HKLM "Software\Classes\.${ext}" ""
  ${If} $0 == "${progId}"
    DeleteRegValue HKLM "Software\Classes\.${ext}" ""
  ${EndIf}
  DeleteRegValue HKLM "Software\Classes\.${ext}\OpenWithProgids" "${progId}"
!macroend

!macro RegisterApplication exe
  ; Register the executable as an application so it appears in "Open with".
  WriteRegStr HKCR "Applications\${exe}" "FriendlyAppName" "Hive Viewer"
  WriteRegStr HKCR "Applications\${exe}\shell\open\command" "" '"$INSTDIR\${exe}" "%1"'
  WriteRegStr HKCR "Applications\${exe}\DefaultIcon" "" '"$INSTDIR\${exe}",0'

  ; Declare supported file types.
  WriteRegStr HKCR "Applications\${exe}\SupportedTypes" ".jpg" ""
  WriteRegStr HKCR "Applications\${exe}\SupportedTypes" ".jpeg" ""
  WriteRegStr HKCR "Applications\${exe}\SupportedTypes" ".png" ""
  WriteRegStr HKCR "Applications\${exe}\SupportedTypes" ".gif" ""
  WriteRegStr HKCR "Applications\${exe}\SupportedTypes" ".webp" ""
  WriteRegStr HKCR "Applications\${exe}\SupportedTypes" ".bmp" ""
  WriteRegStr HKCR "Applications\${exe}\SupportedTypes" ".avif" ""
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; The executable name matches the Cargo package name: "hive-viewer.exe".
  !insertmacro RegisterApplication "hive-viewer.exe"

  !insertmacro RegisterExtension "$INSTDIR\hive-viewer.exe" "jpg" "HiveViewer.JPEG"
  !insertmacro RegisterExtension "$INSTDIR\hive-viewer.exe" "jpeg" "HiveViewer.JPEG"
  !insertmacro RegisterExtension "$INSTDIR\hive-viewer.exe" "png" "HiveViewer.PNG"
  !insertmacro RegisterExtension "$INSTDIR\hive-viewer.exe" "gif" "HiveViewer.GIF"
  !insertmacro RegisterExtension "$INSTDIR\hive-viewer.exe" "webp" "HiveViewer.WebP"
  !insertmacro RegisterExtension "$INSTDIR\hive-viewer.exe" "bmp" "HiveViewer.BMP"
  !insertmacro RegisterExtension "$INSTDIR\hive-viewer.exe" "avif" "HiveViewer.AVIF"

  ; Register the sparse identity package so the app appears in the Windows 11
  ; context menu. Non-fatal on failure: the classic registrations above still work.
  nsExec::ExecToStack 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\sparse-package.ps1" -Action Install -InstallDir "$INSTDIR"'
  Pop $0
  Pop $1

  ; Refresh Windows icon cache so file icons update immediately.
  System::Call 'shell32.dll::SHChangeNotify(i, i, i, i) v (0x08000000, 0, 0, 0)'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Unregister the sparse identity package while its files still exist.
  nsExec::ExecToStack 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\sparse-package.ps1" -Action Uninstall -InstallDir "$INSTDIR"'
  Pop $0
  Pop $1
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Remove ProgId registrations and any extension default we may have set.
  !insertmacro UnregisterExtension "jpg" "HiveViewer.JPEG"
  !insertmacro UnregisterExtension "jpeg" "HiveViewer.JPEG"
  !insertmacro UnregisterExtension "png" "HiveViewer.PNG"
  !insertmacro UnregisterExtension "gif" "HiveViewer.GIF"
  !insertmacro UnregisterExtension "webp" "HiveViewer.WebP"
  !insertmacro UnregisterExtension "bmp" "HiveViewer.BMP"
  !insertmacro UnregisterExtension "avif" "HiveViewer.AVIF"

  DeleteRegKey HKCR "HiveViewer.JPEG"
  DeleteRegKey HKCR "HiveViewer.PNG"
  DeleteRegKey HKCR "HiveViewer.GIF"
  DeleteRegKey HKCR "HiveViewer.WebP"
  DeleteRegKey HKCR "HiveViewer.BMP"
  DeleteRegKey HKCR "HiveViewer.AVIF"

  ; Remove application registration.
  DeleteRegKey HKCR "Applications\hive-viewer.exe"

  ; Refresh Windows icon cache.
  System::Call 'shell32.dll::SHChangeNotify(i, i, i, i) v (0x08000000, 0, 0, 0)'
!macroend
