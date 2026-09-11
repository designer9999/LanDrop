; Protocol-only toasts need a stub CLSID on their AUMID shortcut so Windows
; Notification Center retains them after the process exits. No COM server is
; registered: all actions activate our strictly validated private URI scheme.
; Tauri's NSIS template already includes Win\COM.nsh and Win\Propkey.nsh.
!macro LandropSetToastActivator shortcut
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r0 ""
  ${If} $0 P<> 0
    ${IUnknown::QueryInterface} $0 '("${IID_IPersistFile}",.r1)'
    ${If} $1 P<> 0
      ${IPersistFile::Load} $1 '("${shortcut}", ${STGM_READWRITE})'
      ${IUnknown::QueryInterface} $0 '("${IID_IPropertyStore}",.r2)'
      ${If} $2 P<> 0
        ; A fixed LanDrop-only GUID; this is deliberately not a COM server.
        System::Call '*(&g16 "{736C57A2-94D4-4B36-86CE-9C1C8ECA9C55}")p.r3'
        System::Call '*${SYSSTRUCT_PROPERTYKEY}(${PKEY_AppUserModel_ToastActivatorCLSID})p.r4'
        System::Call '*${SYSSTRUCT_PROPVARIANT}(${VT_CLSID},,p $3)p.r5'
        ${IPropertyStore::SetValue} $2 '($4,$5)'
        System::Free $3
        System::Free $4
        System::Free $5
        ${IPropertyStore::Commit} $2 ""
        ${IUnknown::Release} $2 ""
        ${IPersistFile::Save} $1 '("${shortcut}",1)'
      ${EndIf}
      ${IUnknown::Release} $1 ""
    ${EndIf}
    ${IUnknown::Release} $0 ""
  ${EndIf}
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend

; Add Windows Firewall rule after installation so LanDrop can accept
; incoming LAN connections without the user having to manually allow it.
!macro NSIS_HOOK_POSTINSTALL
  ; Both executable and URI must remain separately quoted. No cmd.exe or
  ; PowerShell is involved. SHCTX follows the installer's user/machine scope.
  WriteRegStr SHCTX "Software\Classes\landrop-notification" "" "URL:LanDrop notification"
  WriteRegStr SHCTX "Software\Classes\landrop-notification" "URL Protocol" ""
  WriteRegStr SHCTX "Software\Classes\landrop-notification\DefaultIcon" "" '"$INSTDIR\landrop.exe",0'
  WriteRegStr SHCTX "Software\Classes\landrop-notification\shell\open\command" "" '"$INSTDIR\landrop.exe" "%1"'
  !if "${STARTMENUFOLDER}" != ""
    !insertmacro LandropSetToastActivator "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  !else
    !insertmacro LandropSetToastActivator "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !endif
  ; Add inbound TCP rule for LanDrop
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="LanDrop TCP" dir=in action=allow program="$INSTDIR\landrop.exe" protocol=TCP localport=29171 profile=private enable=yes'
  ; Add inbound UDP rule for mDNS discovery
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="LanDrop mDNS" dir=in action=allow program="$INSTDIR\landrop.exe" protocol=UDP localport=5353 profile=private enable=yes'
!macroend

; Remove firewall rules on uninstall
!macro NSIS_HOOK_PREUNINSTALL
  ; Do not remove a scheme subsequently registered by a different installation.
  ReadRegStr $0 SHCTX "Software\Classes\landrop-notification\shell\open\command" ""
  ${If} $0 == '"$INSTDIR\landrop.exe" "%1"'
    DeleteRegKey SHCTX "Software\Classes\landrop-notification"
  ${EndIf}
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="LanDrop TCP" program="$INSTDIR\landrop.exe"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="LanDrop mDNS" program="$INSTDIR\landrop.exe"'
  ; Clean up the broad legacy rule used by releases before the rules were scoped.
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="LanDrop" program="$INSTDIR\landrop.exe"'
!macroend
