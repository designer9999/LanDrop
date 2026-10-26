//! Bounded app-presence probes over the installed Tailscale connection.
//! Windows inventories OS routes through notifications, never CLI or LocalAPI.
#[cfg(any(not(target_os = "windows"), test))]
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

pub const DISCOVERY_COOLDOWN: Duration = Duration::from_secs(30);
pub const PROBE_CONCURRENCY: usize = 4;
const SWEEP_BUDGET: usize = 16;
/// Eligible candidates are real tailnet devices only (service nodes are excluded
/// before scheduling), so a short cap lets a colleague who comes online appear
/// within about two minutes without meaningful traffic.
const MAX_BACKOFF: Duration = Duration::from_secs(2 * 60);

#[derive(Default)]
pub struct ProbeSchedule {
    attempts: HashMap<Ipv4Addr, Attempt>,
    priority: HashSet<Ipv4Addr>,
}

struct Attempt {
    last: Instant,
    due: Instant,
    failures: u32,
}

impl ProbeSchedule {
    /// A valid inbound app discovery request is a hint, not proof that its
    /// listener is reachable. Retry it once per reservation window at most.
    pub fn request_probe(&mut self, ip: Ipv4Addr, now: Instant) -> bool {
        if let Some(attempt) = self.attempts.get_mut(&ip) {
            if now.saturating_duration_since(attempt.last) < DISCOVERY_COOLDOWN {
                return false;
            }
            attempt.due = now;
        }
        // Match the bounded hint channel; selection also prunes stale entries.
        if self.priority.len() < 64 {
            self.priority.insert(ip);
        }
        true
    }
    /// Continue a bounded backlog promptly, but never spin on an empty sweep.
    /// A modest in-memory wakeup also notices peers lost by the route monitor;
    /// this does not query the OS or Tailscale service.
    pub fn retry_delay(
        &self,
        eligible: &HashSet<Ipv4Addr>,
        confirmed: &HashSet<Ipv4Addr>,
        now: Instant,
    ) -> Duration {
        eligible
            .iter()
            .filter(|ip| !confirmed.contains(ip))
            .map(|ip| {
                self.attempts
                    .get(ip)
                    .map_or(Duration::ZERO, |a| a.due.saturating_duration_since(now))
            })
            .min()
            .unwrap_or(DISCOVERY_COOLDOWN)
            .clamp(Duration::from_secs(1), DISCOVERY_COOLDOWN)
    }

    /// Oldest attempts first: a large tailnet cannot starve later addresses.
    /// Confirmed live peers are maintained separately by the route monitor.
    #[cfg(any(not(target_os = "windows"), test))]
    pub fn select(
        &mut self,
        eligible: &HashSet<Ipv4Addr>,
        confirmed: &HashSet<Ipv4Addr>,
        now: Instant,
    ) -> Vec<Ipv4Addr> {
        self.select_admitted(eligible, confirmed, now, |_| true)
    }

    /// Admission can separately cap unresolved service-node candidates without
    /// allowing them to starve known colleague addresses later in the queue.
    pub fn select_admitted(
        &mut self,
        eligible: &HashSet<Ipv4Addr>,
        confirmed: &HashSet<Ipv4Addr>,
        now: Instant,
        mut admit: impl FnMut(Ipv4Addr) -> bool,
    ) -> Vec<Ipv4Addr> {
        self.attempts.retain(|ip, _| eligible.contains(ip));
        self.priority
            .retain(|ip| eligible.contains(ip) && !confirmed.contains(ip));
        let mut candidates: Vec<_> = eligible
            .iter()
            .copied()
            .filter(|ip| !confirmed.contains(ip))
            .filter(|ip| {
                self.attempts
                    .get(ip)
                    .is_none_or(|attempt| attempt.due <= now)
            })
            .collect();
        candidates.sort_by_key(|ip| {
            (
                !self.priority.contains(ip),
                self.attempts.get(ip).map(|a| a.last),
                *ip,
            )
        });
        candidates.retain(|ip| admit(*ip));
        candidates.truncate(SWEEP_BUDGET);
        // Reserve immediately so an interrupted probe cannot cause a hot retry.
        for ip in &candidates {
            self.priority.remove(ip);
            let failures = self.attempts.get(ip).map_or(0, |a| a.failures);
            self.attempts.insert(
                *ip,
                Attempt {
                    last: now,
                    due: now + DISCOVERY_COOLDOWN,
                    failures,
                },
            );
        }
        candidates
    }

    pub fn record(&mut self, ip: Ipv4Addr, success: bool, now: Instant) {
        let Some(attempt) = self.attempts.get_mut(&ip) else {
            return;
        };
        attempt.failures = if success {
            0
        } else {
            attempt.failures.saturating_add(1)
        };
        let delay = DISCOVERY_COOLDOWN
            .saturating_mul(1 << attempt.failures.min(5))
            .min(MAX_BACKOFF);
        attempt.due = now + delay;
    }
}

#[cfg(any(not(target_os = "windows"), test))]
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    backend_state: String,
    #[serde(default)]
    peer: Option<HashMap<String, Peer>>,
}

#[cfg(any(not(target_os = "windows"), test))]
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Peer {
    #[serde(default)]
    online: bool,
    #[serde(default, rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
    #[serde(default, rename = "DNSName")]
    dns_name: String,
    #[serde(default)]
    exit_node_option: bool,
    #[serde(default)]
    location: Option<serde_json::Value>,
}

#[cfg(any(not(target_os = "windows"), test))]
impl Peer {
    fn is_service_node(&self) -> bool {
        let dns = self.dns_name.trim_end_matches('.').to_ascii_lowercase();
        dns == "mullvad.ts.net"
            || dns.ends_with(".mullvad.ts.net")
            || (self.exit_node_option && self.location.is_some())
    }
}

pub fn is_tailscale_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    a == 100 && (64..=127).contains(&b)
}

#[cfg(any(not(target_os = "windows"), test))]
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
        .filter(|peer| peer.online && !peer.is_service_node())
        .flat_map(|peer| &peer.tailscale_ips)
        .filter_map(|ip| ip.parse::<Ipv4Addr>().ok())
        .filter(|ip| is_tailscale_ipv4(*ip))
        .collect())
}

/// Own the inventory worker for this discovery lifetime. Windows deliberately
/// has no `online_peers` CLI entry point to accidentally call.
pub async fn run_inventory(
    cancel: tokio_util::sync::CancellationToken,
    tx: tokio::sync::watch::Sender<Result<HashSet<Ipv4Addr>, String>>,
) {
    #[cfg(target_os = "windows")]
    super::windows_tailnet::run(cancel, tx).await;

    #[cfg(not(target_os = "windows"))]
    loop {
        let result = tokio::select! {
            _ = cancel.cancelled() => return,
            result = online_peers() => result,
        };
        if tx.send(result).is_err() {
            return;
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = mobile::wakeup().notified() => {}
            _ = tokio::time::sleep(DISCOVERY_COOLDOWN) => {}
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(DISCOVERY_COOLDOWN) => {}
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "android", target_os = "ios")))]
pub async fn online_peers() -> Result<HashSet<Ipv4Addr>, String> {
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    #[allow(unused_mut)]
    let mut candidates = vec![PathBuf::from("tailscale")];
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
    if local_tunnel_ipv4().is_none() {
        return Err("Tailscale is not connected on this device".into());
    }
    Ok(mobile::candidates())
}

#[cfg(any(not(target_os = "windows"), test))]
/// Interface names Tailscale gives its tunnel: `tun*` (Android VpnService),
/// `tailscale*` (Linux), `utun*` (macOS). Cellular and Wi-Fi interfaces can also
/// carry carrier-grade 100.64.0.0/10 addresses, so the name matters.
fn is_tunnel_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("tun") || name.starts_with("tailscale") || name.starts_with("utun")
}

/// This device's Tailscale IPv4 address, if its tunnel interface is up.
#[cfg(not(target_os = "windows"))]
pub fn local_tunnel_ipv4() -> Option<Ipv4Addr> {
    tunnel_ipv4_from(local_ip_address::list_afinet_netifas().unwrap_or_default())
}

#[cfg(any(not(target_os = "windows"), test))]
fn tunnel_ipv4_from(interfaces: Vec<(String, std::net::IpAddr)>) -> Option<Ipv4Addr> {
    interfaces.into_iter().find_map(|(name, ip)| match ip {
        std::net::IpAddr::V4(ip) if is_tailscale_ipv4(ip) && is_tunnel_interface(&name) => Some(ip),
        _ => None,
    })
}

/// A connection that reached this device's own Tailscale tunnel address from a
/// tailnet address came through Tailscale, which only delivers packets from
/// authenticated tailnet nodes allowed by the tailnet policy. A carrier-CGNAT
/// neighbour cannot reach the tunnel address.
#[cfg(any(not(target_os = "windows"), test))]
pub fn arrived_over_tunnel(local: Ipv4Addr, remote: Ipv4Addr, tunnel: Option<Ipv4Addr>) -> bool {
    tunnel == Some(local) && is_tailscale_ipv4(remote) && remote != local
}

/// Mobile Tailscale apps expose no peer list to other apps. Android discovery
/// therefore works from devices that have contacted this one over the tunnel and
/// from devices remembered from earlier sessions; every candidate must still pass
/// the LanDrop identity probe before it becomes visible.
#[cfg(any(target_os = "android", target_os = "ios", test))]
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
pub mod mobile {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    const MAX_REMEMBERED: usize = 32;
    const FORGET_AFTER_SECS: u64 = 90 * 24 * 60 * 60;

    #[derive(Default, serde::Serialize, serde::Deserialize)]
    pub struct Remembered {
        /// Tailscale IPv4 → last time a LanDrop identity answered there (unix s).
        peers: HashMap<Ipv4Addr, u64>,
    }

    impl Remembered {
        pub fn ips(&self) -> HashSet<Ipv4Addr> {
            self.peers.keys().copied().collect()
        }

        /// Returns true when the address was not known before.
        pub fn remember(&mut self, ip: Ipv4Addr, now: u64) -> bool {
            if !is_tailscale_ipv4(ip) {
                return false;
            }
            let new = self.peers.insert(ip, now).is_none();
            self.peers
                .retain(|_, seen| now.saturating_sub(*seen) <= FORGET_AFTER_SECS);
            while self.peers.len() > MAX_REMEMBERED {
                let Some(oldest) = self
                    .peers
                    .iter()
                    .min_by_key(|(_, seen)| **seen)
                    .map(|(ip, _)| *ip)
                else {
                    break;
                };
                self.peers.remove(&oldest);
            }
            new
        }
    }

    struct Store {
        path: Option<PathBuf>,
        remembered: Remembered,
    }

    fn store() -> &'static Mutex<Store> {
        static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
        STORE.get_or_init(|| {
            Mutex::new(Store {
                path: None,
                remembered: Remembered::default(),
            })
        })
    }

    pub fn wakeup() -> &'static tokio::sync::Notify {
        static WAKE: OnceLock<tokio::sync::Notify> = OnceLock::new();
        WAKE.get_or_init(tokio::sync::Notify::new)
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs())
    }

    /// Load remembered peers from the app data directory (once per launch).
    pub fn init(data_dir: &std::path::Path) {
        let path = data_dir.join("tailnet_peers.json");
        let remembered = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if let Ok(mut store) = store().lock() {
            store.path = Some(path);
            store.remembered = remembered;
        }
    }

    /// Record a tailnet address that reached or answered this device. New
    /// addresses wake discovery so the reciprocal identity check runs promptly.
    pub fn remember(ip: Ipv4Addr) {
        let Ok(mut store) = store().lock() else {
            return;
        };
        let new = store.remembered.remember(ip, now());
        if let (Some(path), Ok(json)) = (&store.path, serde_json::to_vec(&store.remembered)) {
            let _ = crate::path_utils::write_atomic(path, &json);
        }
        drop(store);
        if new {
            wakeup().notify_one();
        }
    }

    pub fn candidates() -> HashSet<Ipv4Addr> {
        store()
            .lock()
            .map(|store| store.remembered.ips())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_address_requires_a_tunnel_interface() {
        let carrier = (
            "rmnet_data1".to_string(),
            std::net::IpAddr::V4(Ipv4Addr::new(100, 72, 1, 9)),
        );
        let wifi = (
            "wlan0".to_string(),
            std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 0, 20)),
        );
        let tunnel = (
            "tun0".to_string(),
            std::net::IpAddr::V4(Ipv4Addr::new(100, 104, 195, 26)),
        );
        assert_eq!(tunnel_ipv4_from(vec![carrier.clone(), wifi.clone()]), None);
        assert_eq!(
            tunnel_ipv4_from(vec![carrier, wifi, tunnel]),
            Some(Ipv4Addr::new(100, 104, 195, 26))
        );
        assert!(is_tunnel_interface("tailscale0") && is_tunnel_interface("utun4"));
    }

    #[test]
    fn only_connections_to_our_tunnel_address_count_as_tailnet() {
        let phone = Ipv4Addr::new(100, 104, 195, 26);
        let tunnel = Some(phone);
        let pc = Ipv4Addr::new(100, 119, 196, 75);
        assert!(arrived_over_tunnel(phone, pc, tunnel));
        // Carrier CGNAT address on the cellular interface, not the tunnel.
        assert!(!arrived_over_tunnel(
            Ipv4Addr::new(100, 72, 1, 9),
            pc,
            tunnel
        ));
        assert!(!arrived_over_tunnel(
            phone,
            Ipv4Addr::new(192, 168, 0, 5),
            tunnel
        ));
        assert!(!arrived_over_tunnel(phone, pc, None));
    }

    #[test]
    fn remembered_tailnet_peers_are_bounded_and_expire() {
        let mut remembered = mobile::Remembered::default();
        assert!(remembered.remember(Ipv4Addr::new(100, 64, 0, 1), 1_000));
        assert!(!remembered.remember(Ipv4Addr::new(100, 64, 0, 1), 2_000));
        assert!(!remembered.remember(Ipv4Addr::new(192, 168, 0, 1), 2_000));
        for i in 0..40u8 {
            remembered.remember(Ipv4Addr::new(100, 65, 0, i), 3_000 + u64::from(i));
        }
        assert_eq!(remembered.ips().len(), 32);
        // Ninety days later everything not seen since is forgotten.
        remembered.remember(Ipv4Addr::new(100, 66, 0, 1), 3_000 + 91 * 24 * 60 * 60);
        assert_eq!(
            remembered.ips(),
            HashSet::from([Ipv4Addr::new(100, 66, 0, 1)])
        );
    }

    #[test]
    fn retry_timer_is_bounded_and_new_addresses_take_priority() {
        let now = Instant::now();
        let old = Ipv4Addr::new(100, 64, 0, 1);
        let new = Ipv4Addr::new(100, 64, 0, 2);
        let mut eligible = HashSet::from([old]);
        let confirmed = HashSet::new();
        let mut schedule = ProbeSchedule::default();
        assert_eq!(
            schedule.retry_delay(&eligible, &confirmed, now),
            Duration::from_secs(1)
        );
        schedule.select(&eligible, &confirmed, now);
        schedule.record(old, false, now);
        assert_eq!(
            schedule.retry_delay(&eligible, &confirmed, now),
            DISCOVERY_COOLDOWN
        );
        eligible.insert(new);
        assert_eq!(schedule.select(&eligible, &confirmed, now), [new]);
        assert_eq!(
            schedule.retry_delay(&HashSet::new(), &confirmed, now),
            DISCOVERY_COOLDOWN
        );
    }

    #[test]
    fn reciprocal_hint_cannot_bypass_probe_reservation_or_create_a_peer() {
        let now = Instant::now();
        let ip = Ipv4Addr::new(100, 64, 0, 1);
        let eligible = HashSet::from([ip]);
        let mut schedule = ProbeSchedule::default();
        schedule.select(&eligible, &HashSet::new(), now);
        schedule.record(ip, false, now);
        assert!(!schedule.request_probe(ip, now + Duration::from_secs(1)));
        assert!(schedule
            .select(&eligible, &HashSet::new(), now + Duration::from_secs(1))
            .is_empty());
        assert!(schedule.request_probe(ip, now + DISCOVERY_COOLDOWN));
        assert!(schedule
            .select(&eligible, &eligible, now + DISCOVERY_COOLDOWN)
            .is_empty());
        assert_eq!(
            schedule.select(&eligible, &HashSet::new(), now + DISCOVERY_COOLDOWN),
            [ip]
        );
        assert!(!schedule.request_probe(ip, now + DISCOVERY_COOLDOWN));
    }

    #[test]
    fn capped_unknown_admission_does_not_starve_known_candidates() {
        let now = Instant::now();
        let eligible: HashSet<_> = (1..=40).map(|n| Ipv4Addr::new(100, 64, 0, n)).collect();
        let mut schedule = ProbeSchedule::default();
        let mut unknown_budget = 4;
        let selected = schedule.select_admitted(&eligible, &HashSet::new(), now, |ip| {
            if ip.octets()[3] >= 30 {
                return true;
            }
            if unknown_budget == 0 {
                return false;
            }
            unknown_budget -= 1;
            true
        });
        assert_eq!(selected.iter().filter(|ip| ip.octets()[3] < 30).count(), 4);
        assert!(selected.contains(&Ipv4Addr::new(100, 64, 0, 40)));
        assert!(selected.len() <= SWEEP_BUDGET);
    }

    #[test]
    fn reciprocal_hint_prioritizes_a_friend_without_bypassing_unknown_budget() {
        let now = Instant::now();
        let eligible: HashSet<_> = (1..=200).map(|n| Ipv4Addr::new(100, 64, 0, n)).collect();
        let friend = Ipv4Addr::new(100, 64, 0, 200);
        let mut schedule = ProbeSchedule::default();
        assert!(schedule.request_probe(friend, now));
        let mut budget = 4;
        let selected = schedule.select_admitted(&eligible, &HashSet::new(), now, |_| {
            if budget == 0 {
                return false;
            }
            budget -= 1;
            true
        });
        assert_eq!(selected.len(), 4);
        assert_eq!(selected[0], friend);
        assert!(!schedule.priority.contains(&friend));
        assert!(!schedule.request_probe(friend, now));
        for ip in &eligible {
            schedule.request_probe(*ip, now);
        }
        assert!(schedule.priority.len() <= 64);
        schedule.select(&HashSet::new(), &HashSet::new(), now);
        assert!(schedule.priority.is_empty());
    }

    #[test]
    fn excludes_service_nodes_but_keeps_colleague_exit_nodes() {
        let parsed = parse_status(br#"{"BackendState":"Running","Peer":{
            "mullvad":{"Online":true,"DNSName":"se-sto.MULLVAD.ts.net.","TailscaleIPs":["100.64.0.1"]},
            "location":{"Online":true,"ExitNodeOption":true,"Location":{"Country":"SE"},"TailscaleIPs":["100.64.0.2"]},
            "colleague":{"Online":true,"ExitNodeOption":true,"Location":null,"DNSName":"pc.tailnet.ts.net.","TailscaleIPs":["100.64.0.3"]},
            "suffix_boundary":{"Online":true,"DNSName":"notmullvad.ts.net.","TailscaleIPs":["100.64.0.4"]},
            "spoofed_suffix":{"Online":true,"DNSName":"mullvad.ts.net.example.org.","TailscaleIPs":["100.64.0.5"]}
        }}"#).unwrap();
        assert_eq!(
            parsed,
            [3, 4, 5].map(|n| Ipv4Addr::new(100, 64, 0, n)).into()
        );
    }

    #[test]
    fn sweep_budget_is_fair_and_excludes_confirmed_peers() {
        let now = Instant::now();
        let eligible: HashSet<_> = (1..=40).map(|n| Ipv4Addr::new(100, 64, 0, n)).collect();
        let confirmed = HashSet::from([Ipv4Addr::new(100, 64, 0, 1)]);
        let mut schedule = ProbeSchedule::default();
        let mut attempted = HashSet::new();
        for cycle in 0..3 {
            let selected = schedule.select(&eligible, &confirmed, now + DISCOVERY_COOLDOWN * cycle);
            assert!(selected.len() <= SWEEP_BUDGET);
            assert!(selected.iter().all(|ip| !confirmed.contains(ip)));
            attempted.extend(selected);
        }
        assert_eq!(attempted.len(), 39);
    }

    #[test]
    fn failures_back_off_exponentially_and_success_resets_delay() {
        let ip = Ipv4Addr::new(100, 64, 0, 1);
        let eligible = HashSet::from([ip]);
        let mut now = Instant::now();
        let mut schedule = ProbeSchedule::default();
        for seconds in [60, 120, 120, 120] {
            assert_eq!(schedule.select(&eligible, &HashSet::new(), now), vec![ip]);
            schedule.record(ip, false, now);
            let delay = Duration::from_secs(seconds);
            assert!(schedule
                .select(
                    &eligible,
                    &HashSet::new(),
                    now + delay - Duration::from_millis(1)
                )
                .is_empty());
            now += delay;
        }
        assert_eq!(schedule.select(&eligible, &HashSet::new(), now), vec![ip]);
        schedule.record(ip, true, now);
        assert!(schedule.select(&eligible, &HashSet::new(), now).is_empty());
        assert_eq!(
            schedule.select(&eligible, &HashSet::new(), now + DISCOVERY_COOLDOWN),
            vec![ip]
        );
    }

    #[test]
    fn departed_candidates_are_pruned_and_lost_confirmed_peers_can_retry() {
        let ip = Ipv4Addr::new(100, 64, 0, 1);
        let eligible = HashSet::from([ip]);
        let now = Instant::now();
        let mut schedule = ProbeSchedule::default();
        schedule.select(&eligible, &HashSet::new(), now);
        schedule.record(ip, true, now);
        assert!(schedule
            .select(&eligible, &eligible, now + DISCOVERY_COOLDOWN)
            .is_empty());
        assert_eq!(
            schedule.select(&eligible, &HashSet::new(), now + DISCOVERY_COOLDOWN),
            vec![ip]
        );
        schedule.select(&HashSet::new(), &HashSet::new(), now);
        assert!(schedule.attempts.is_empty());
        assert_eq!(schedule.select(&eligible, &HashSet::new(), now), vec![ip]);
    }

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
        assert!(parse_status(br#"{"BackendState":"NoState"}"#).is_err());
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
