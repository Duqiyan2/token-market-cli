; This marker belongs only to the reviewed current-user NSIS lifecycle contract.
; The release workflow pins Tauri CLI 2.12.4 / bundler 2.10.4.
!macro NSIS_HOOK_POSTINSTALL
  ; Tauri includes hooks before defining INSTALLMODE/UNINSTKEY. Evaluate the
  ; contract only when this macro is expanded after those definitions exist.
  !if "${INSTALLMODE}" != "currentUser"
    !error "Token Market automatic updates require NSIS currentUser"
  !endif
  WriteRegStr HKCU "${UNINSTKEY}" "TokenMarketUpdatePolicy" "nsis-currentUser-v1-bundler2.10.4"
  WriteRegStr HKCU "${UNINSTKEY}" "TokenMarketUpdateDirectory" "$INSTDIR"
!macroend
