//! Windows Firewall helper for LAN and Tailscale reception.
//!
//! LanDrop installs per user, so the installer cannot add firewall rules, and
//! Windows' first-run prompt only allows private networks by default. Windows
//! often classifies the Tailscale adapter as a public network, which silently
//! blocks colleagues on the tailnet. This adds one narrow inbound rule, with the
//! user's explicit UAC approval: LanDrop's executable, TCP 29171 (and mDNS on the
//! LAN), only from the local subnet and the Tailscale range, on any profile.

#[cfg(target_os = "windows")]
const TCP_RULE: &str = "LanDrop (LAN and Tailscale)";
#[cfg(target_os = "windows")]
const MDNS_RULE: &str = "LanDrop discovery (LAN)";

#[derive(serde::Serialize)]
pub struct FirewallStatus {
    /// The helper exists on this platform.
    supported: bool,
    /// LanDrop's LAN + Tailscale inbound rule is present.
    configured: bool,
}

#[cfg(target_os = "windows")]
fn rule_exists(name: &str) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            &format!("name={name}"),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The elevated command line: replace earlier copies, then add both rules.
#[cfg(any(target_os = "windows", test))]
fn rule_script(exe: &str) -> Result<String, String> {
    if exe.contains('"') || exe.contains('&') || exe.contains('%') || exe.is_empty() {
        return Err("Unexpected characters in the LanDrop install path".into());
    }
    let tcp = "LanDrop (LAN and Tailscale)";
    let mdns = "LanDrop discovery (LAN)";
    Ok(format!(
        "netsh advfirewall firewall delete rule name=\"{tcp}\" >nul 2>&1 & \
         netsh advfirewall firewall delete rule name=\"{mdns}\" >nul 2>&1 & \
         netsh advfirewall firewall add rule name=\"{tcp}\" dir=in action=allow \
         program=\"{exe}\" protocol=TCP localport=29171 \
         remoteip=LocalSubnet,100.64.0.0/10 profile=any enable=yes && \
         netsh advfirewall firewall add rule name=\"{mdns}\" dir=in action=allow \
         program=\"{exe}\" protocol=UDP localport=5353 remoteip=LocalSubnet \
         profile=any enable=yes"
    ))
}

#[tauri::command]
pub async fn firewall_status() -> FirewallStatus {
    #[cfg(target_os = "windows")]
    {
        let configured =
            tokio::task::spawn_blocking(|| rule_exists(TCP_RULE) && rule_exists(MDNS_RULE))
                .await
                .unwrap_or(false);
        FirewallStatus {
            supported: true,
            configured,
        }
    }
    #[cfg(not(target_os = "windows"))]
    FirewallStatus {
        supported: false,
        configured: false,
    }
}

/// Ask Windows (UAC) to add LanDrop's inbound rules. Rejects with "cancelled"
/// when the user declines the prompt.
#[tauri::command]
pub async fn allow_through_firewall() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe()
            .map_err(|e| format!("Cannot locate LanDrop: {e}"))?
            .to_string_lossy()
            .into_owned();
        let script = rule_script(&exe)?;
        tokio::task::spawn_blocking(move || run_elevated(&script))
            .await
            .map_err(|e| e.to_string())??;
        if rule_exists(TCP_RULE) {
            Ok(())
        } else {
            Err("Windows did not add the firewall rule".into())
        }
    }
    #[cfg(not(target_os = "windows"))]
    Err("The firewall helper is only available on Windows".into())
}

#[cfg(target_os = "windows")]
#[expect(
    unsafe_code,
    reason = "UAC elevation requires the ShellExecuteExW FFI call"
)]
fn run_elevated(script: &str) -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    // /s keeps the inner quotes literal once the outer pair is stripped.
    let parameters = HSTRING::from(format!("/d /s /c \"{script}\""));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: w!("cmd.exe"),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: `info` and the strings it points to outlive the call.
    if let Err(error) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if error.code() == ERROR_CANCELLED.to_hresult() {
            "cancelled".into()
        } else {
            format!("Could not ask Windows for permission: {error}")
        });
    }
    if info.hProcess.is_invalid() {
        return Err("Windows did not start the firewall change".into());
    }
    let mut code = 1u32;
    // SAFETY: hProcess is a valid process handle owned by us until CloseHandle.
    unsafe {
        let waited = WaitForSingleObject(info.hProcess, INFINITE);
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        if waited != WAIT_OBJECT_0 {
            return Err("Waiting for the firewall change failed".into());
        }
    }
    if code == 0 {
        Ok(())
    } else {
        Err(format!("netsh reported an error (exit code {code})"))
    }
}

#[cfg(test)]
mod tests {
    use super::rule_script;

    #[test]
    fn rule_is_narrow_and_quotes_the_install_path() {
        let script = rule_script(r"C:\Users\A User\AppData\Local\LanDrop\landrop.exe").unwrap();
        assert!(script.contains(r#"program="C:\Users\A User\AppData\Local\LanDrop\landrop.exe""#));
        assert!(script.contains("localport=29171 remoteip=LocalSubnet,100.64.0.0/10 profile=any"));
        assert!(script.contains("localport=5353 remoteip=LocalSubnet profile=any"));
        assert!(!script.contains("remoteip=any"));
    }

    #[test]
    fn rejects_paths_that_could_break_the_command_line() {
        assert!(rule_script(r#"C:\evil" & calc.exe"#).is_err());
        assert!(rule_script("C:\\a%PATH%\\landrop.exe").is_err());
        assert!(rule_script("").is_err());
    }
}
