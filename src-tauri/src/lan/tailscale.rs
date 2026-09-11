//! Desktop discovery uses the already authenticated local Tailscale client.
//! No login, control-server configuration, or tailnet writes are performed.
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    backend_state: String,
    #[serde(default)]
    peer: Option<HashMap<String, Peer>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Peer {
    #[serde(default)]
    online: bool,
    #[serde(default, rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
}

pub fn is_tailscale_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    a == 100 && (64..=127).contains(&b)
}

pub fn parse_status(bytes: &[u8]) -> Result<HashSet<Ipv4Addr>, String> {
    let status: Status = serde_json::from_slice(bytes)
        .map_err(|_| "Tailscale returned an unsupported status response".to_string())?;
    if status.backend_state != "Running" {
        return Err("Tailscale is not connected; connect the desktop Tailscale client".into());
    }
    Ok(status
        .peer
        .unwrap_or_default()
        .values()
        .filter(|peer| peer.online)
        .flat_map(|peer| &peer.tailscale_ips)
        .filter_map(|ip| ip.parse::<Ipv4Addr>().ok())
        .filter(|ip| is_tailscale_ipv4(*ip))
        .collect())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn online_peers() -> Result<HashSet<Ipv4Addr>, String> {
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    #[allow(unused_mut)]
    let mut candidates = vec![PathBuf::from("tailscale")];
    #[cfg(target_os = "windows")]
    {
        // GUI applications often do not inherit the installation's PATH update.
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(program_files).join("Tailscale/tailscale.exe"));
        }
    }
    #[cfg(target_os = "macos")]
    candidates.extend([
        PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale"),
        PathBuf::from("/usr/local/bin/tailscale"),
        PathBuf::from("/opt/homebrew/bin/tailscale"),
    ]);

    for executable in candidates {
        let mut command = tokio::process::Command::new(executable);
        command
            .args(["status", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Cannot run the local Tailscale client".into()),
        };
        let Some(stdout) = child.stdout.take() else {
            return Err("Cannot read Tailscale status".into());
        };
        const MAX_STATUS_BYTES: u64 = 8 * 1024 * 1024;
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            let mut output = Vec::new();
            stdout
                .take(MAX_STATUS_BYTES + 1)
                .read_to_end(&mut output)
                .await
                .map_err(|_| "Cannot read Tailscale status".to_string())?;
            if output.len() as u64 > MAX_STATUS_BYTES {
                return Err("Tailscale status exceeded the size limit".to_string());
            }
            if !child
                .wait()
                .await
                .map_err(|_| "Cannot wait for Tailscale status".to_string())?
                .success()
            {
                return Err(
                    "Tailscale is unavailable; check that its desktop client is connected"
                        .to_string(),
                );
            }
            parse_status(&output)
        })
        .await;
        return result
            .unwrap_or_else(|_| Err("The local Tailscale status command timed out".into()));
    }
    Err(
        "Tailscale CLI not found; install the desktop Tailscale client to discover tailnet devices"
            .into(),
    )
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn online_peers() -> Result<HashSet<Ipv4Addr>, String> {
    Err("Automatic Tailscale discovery is available on desktop".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_only_online_tailnet_ipv4_addresses() {
        let parsed = parse_status(
            br#"{"BackendState":"Running","Peer":{
            "a":{"Online":true,"TailscaleIPs":["100.100.1.2","fd7a:115c:a1e0::1","8.8.8.8"]},
            "b":{"Online":false,"TailscaleIPs":["100.100.1.3"]},
            "c":{"Online":true,"TailscaleIPs":["100.100.1.2"]}}}"#,
        )
        .unwrap();
        assert_eq!(parsed, HashSet::from([Ipv4Addr::new(100, 100, 1, 2)]));
    }

    #[test]
    fn rejects_disconnected_or_invalid_status() {
        assert!(parse_status(br#"{"BackendState":"Stopped"}"#).is_err());
        assert!(parse_status(br#"{"BackendState":"NeedsLogin"}"#).is_err());
        assert!(parse_status(b"invalid").is_err());
        assert!(parse_status(br#"{"BackendState":"Running"}"#)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn does_not_treat_all_100_addresses_as_tailscale() {
        assert!(!is_tailscale_ipv4(Ipv4Addr::new(100, 63, 255, 255)));
        assert!(is_tailscale_ipv4(Ipv4Addr::new(100, 64, 0, 1)));
        assert!(is_tailscale_ipv4(Ipv4Addr::new(100, 127, 255, 254)));
        assert!(!is_tailscale_ipv4(Ipv4Addr::new(100, 128, 0, 1)));
    }
}
