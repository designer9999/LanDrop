//! Passive Windows route inventory; never connects to Tailscale's LocalAPI.
//!
//! Routes are candidates, not online users or cryptographic application identities.
//! Only quad100 PTR classification generates traffic here. Application probes live
//! in discovery. Every consumer must revalidate `binding_for` before using a route.

use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, OnceLock, RwLock},
    time::Duration,
};

use futures::{stream::FuturesUnordered, StreamExt};
use hickory_proto::{
    op::{Message, MessageType, OpCode, Query, ResponseCode},
    rr::{DNSClass, Name, RData, RecordType},
};
use net_route::{Handle, Route, RouteChange};
use tokio::{
    net::UdpSocket,
    sync::{watch, Semaphore},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

const ADAPTER_GUID: &str = "37217669-42da-4657-a55b-0d995d328250";
const QUAD100: Ipv4Addr = Ipv4Addr::new(100, 100, 100, 100);
const MAX_CANDIDATES: usize = 10_000;
const DNS_CONCURRENCY: usize = 8;
const DNS_TIMEOUT: Duration = Duration::from_secs(1);
const DEBOUNCE: Duration = Duration::from_millis(300);
const REPAIR: Duration = Duration::from_secs(300);
// Classification is an application scheduling hint, not a DNS answer cache.
// Reconsider exclusions periodically and on inventory-generation changes.
const CLASSIFICATION_RECHECK: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteBinding {
    pub local_ipv4: Ipv4Addr,
    pub if_index: u32,
    pub generation: u64,
}

#[derive(Default)]
struct Authority {
    generation: u64,
    snapshot: Option<Snapshot>,
    eligible: HashSet<Ipv4Addr>,
    unknown: HashSet<Ipv4Addr>,
    #[cfg(test)]
    classified_count: usize,
    #[cfg(test)]
    mullvad_count: usize,
}

static AUTHORITY: OnceLock<RwLock<Authority>> = OnceLock::new();
static SNAPSHOT_ADMISSION: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn authority() -> &'static RwLock<Authority> {
    AUTHORITY.get_or_init(|| RwLock::new(Authority::default()))
}

/// Callers must bind the Tailscale source address and revalidate this binding
/// after connect. Windows' normal strong-host routing is required; this module
/// neither changes nor certifies custom weak-host routing configurations.
pub fn binding_for(ip: Ipv4Addr) -> Option<RouteBinding> {
    let state = authority().read().ok()?;
    let snapshot = state.snapshot.as_ref()?;
    if Instant::now() >= snapshot.valid_until {
        return None;
    }
    state.eligible.contains(&ip).then(|| RouteBinding {
        local_ipv4: snapshot.local_ipv4[0],
        if_index: snapshot.if_index,
        generation: state.generation,
    })
}

pub fn inbound_allowed(local: Ipv4Addr, remote: Ipv4Addr) -> bool {
    let Ok(state) = authority().read() else {
        return false;
    };
    state.snapshot.as_ref().is_some_and(|snapshot| {
        Instant::now() < snapshot.valid_until
            && snapshot.local_ipv4.contains(&local)
            && state.eligible.contains(&remote)
    })
}

/// Failed PTR lookups need a smaller application-probe budget; DNS is not
/// required for communication and failure must not permanently hide a friend.
pub fn classification_unknown(ip: Ipv4Addr) -> bool {
    authority()
        .read()
        .map_or(true, |state| state.unknown.contains(&ip))
}

fn invalidate() {
    if let Ok(mut state) = authority().write() {
        state.generation = state.generation.wrapping_add(1);
        state.snapshot = None;
        state.eligible.clear();
        state.unknown.clear();
        #[cfg(test)]
        {
            state.classified_count = 0;
            state.mullvad_count = 0;
        }
    }
}

struct ClearOnDrop;

impl Drop for ClearOnDrop {
    fn drop(&mut self) {
        invalidate();
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    if_index: u32,
    local_ipv4: Vec<Ipv4Addr>,
    candidates: HashSet<Ipv4Addr>,
    valid_until: Instant,
}

impl Snapshot {
    fn same_inventory(&self, other: &Self) -> bool {
        self.if_index == other.if_index
            && self.local_ipv4 == other.local_ipv4
            && self.candidates == other.candidates
    }
}

fn is_cgnat(ip: Ipv4Addr) -> bool {
    let bytes = ip.octets();
    bytes[0] == 100 && (64..=127).contains(&bytes[1])
}

fn matches_guid(name: &str) -> bool {
    name.trim_matches(['{', '}'])
        .eq_ignore_ascii_case(ADAPTER_GUID)
}

fn select_snapshot(interfaces: &[netdev::Interface], routes: &[Route]) -> Result<Snapshot, String> {
    let mut matching = interfaces.iter().filter(|interface| {
        matches_guid(&interface.name) && interface.is_oper_up() && interface.is_up()
    });
    let interface = matching.next().ok_or_else(|| {
        "Tailscale is disconnected or its Windows adapter is unavailable.".to_owned()
    })?;
    if matching.next().is_some() {
        return Err("Tailscale adapter identity is ambiguous.".to_owned());
    }
    let mut local_ipv4: Vec<_> = interface
        .ipv4_addrs()
        .into_iter()
        .filter(|ip| is_cgnat(*ip) && *ip != QUAD100)
        .collect();
    local_ipv4.sort_unstable();
    local_ipv4.dedup();
    if local_ipv4.is_empty() {
        return Err("Automatic discovery needs Tailscale IPv4 peer routes; IPv6-only discovery is unavailable.".to_owned());
    }
    let mut candidates = HashSet::new();
    for route in routes
        .iter()
        .filter(|route| route.ifindex == Some(interface.index))
    {
        let IpAddr::V4(ip) = route.destination else {
            continue;
        };
        if !is_cgnat(ip) {
            continue;
        }
        if route.prefix != 32 {
            // Never expand /10 or advertised subnets into address-space scans.
            if route.prefix <= 10 {
                return Err("Tailscale exposes aggregated routes, so safe automatic peer discovery is unavailable.".to_owned());
            }
            continue;
        }
        if ip != QUAD100 && !local_ipv4.contains(&ip) {
            candidates.insert(ip);
        }
        if candidates.len() > MAX_CANDIDATES {
            return Err(
                "Tailscale peer-route inventory exceeds the discovery safety limit.".to_owned(),
            );
        }
    }
    Ok(Snapshot {
        if_index: interface.index,
        local_ipv4,
        candidates,
        // A failed or stalled repair must not leave admission valid forever.
        // DNS publications reuse this deadline rather than renewing it.
        valid_until: Instant::now() + REPAIR + Duration::from_secs(5),
    })
}

async fn read_snapshot(handle: Arc<Handle>) -> Result<Snapshot, String> {
    // Both libraries call finite, read-only Windows IP Helper APIs. Keep them off
    // Tokio's executor threads. One request is in flight; cancellation never
    // starts another. Windows API calls themselves cannot be forcibly cancelled.
    let permit = SNAPSHOT_ADMISSION
        .get_or_init(|| Arc::new(Semaphore::new(1)))
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "Windows route reader is unavailable.".to_owned())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let interfaces = netdev::get_interfaces();
        let routes = futures::executor::block_on(handle.list())
            .map_err(|_| "Windows route inventory could not be read.".to_owned())?;
        select_snapshot(&interfaces, &routes)
    })
    .await
    .map_err(|_| "Windows route inventory worker stopped.".to_owned())?
}

fn relevant_event(event: &RouteChange, snapshot: Option<&Snapshot>) -> bool {
    if snapshot.is_none() {
        return true;
    }
    let route = match event {
        RouteChange::Add(route) | RouteChange::Delete(route) | RouteChange::Change(route) => route,
    };
    // This provider handles IPv4 exact peer routes only. Changes to internet
    // defaults, Wi-Fi subnets or unrelated IPv6 routes cannot change an exact
    // Tailscale /32 plus explicitly bound Tailscale source. CGNAT changes on ANY
    // interface remain relevant, including a competing more-specific route.
    matches!(route.destination, IpAddr::V4(ip) if is_cgnat(ip))
}

#[derive(Clone)]
struct Classification {
    // None means not queried yet. Some(false) includes bounded DNS failures;
    // application reachability still has to be proved before showing a user.
    mullvad: Option<bool>,
    next_query: Instant,
    failures: u8,
}

fn is_mullvad_name(name: &str) -> bool {
    name.trim_end_matches('.')
        .to_ascii_lowercase()
        .ends_with(".mullvad.ts.net")
}

fn reverse_name(ip: Ipv4Addr) -> Result<Name, ()> {
    let [a, b, c, d] = ip.octets();
    Name::from_ascii(format!("{d}.{c}.{b}.{a}.in-addr.arpa.")).map_err(|_| ())
}

fn parse_ptr_response(bytes: &[u8], id: u16, query: &Query) -> Option<bool> {
    let response = Message::from_vec(bytes).ok()?;
    if response.id != id
        || response.message_type != MessageType::Response
        || response.op_code != OpCode::Query
        || response.truncation
        || response.response_code != ResponseCode::NoError
        || response.queries != [query.clone()]
    {
        return None;
    }
    response.answers.iter().find_map(|record| {
        if &record.name != query.name() || record.dns_class != DNSClass::IN {
            return None;
        }
        match &record.data {
            RData::PTR(name) => Some(is_mullvad_name(&name.to_utf8())),
            _ => None,
        }
    })
}

async fn classify_ptr(ip: Ipv4Addr, local: Ipv4Addr) -> (Ipv4Addr, Option<bool>) {
    let result = tokio::time::timeout(DNS_TIMEOUT, async {
        let socket = UdpSocket::bind((local, 0)).await.ok()?;
        // Connected UDP verifies the response source. There is deliberately no
        // system resolver, public resolver, search suffix or TCP fallback.
        socket.connect((QUAD100, 53)).await.ok()?;
        let random = uuid::Uuid::new_v4();
        let id = u16::from_be_bytes([random.as_bytes()[0], random.as_bytes()[1]]);
        let query = Query::query(reverse_name(ip).ok()?, RecordType::PTR);
        let mut message = Message::new(id, MessageType::Query, OpCode::Query);
        message.metadata.recursion_desired = false;
        message.add_query(query.clone());
        let bytes = message.to_vec().ok()?;
        socket.send(&bytes).await.ok()?;
        let mut buffer = [0u8; 4096];
        let length = socket.recv(&mut buffer).await.ok()?;
        parse_ptr_response(&buffer[..length], id, &query)
    })
    .await
    .ok()
    .flatten();
    (ip, result)
}

fn publish(
    snapshot: &Snapshot,
    cache: &HashMap<Ipv4Addr, Classification>,
    tx: &watch::Sender<Result<HashSet<Ipv4Addr>, String>>,
) {
    let eligible: HashSet<_> = cache
        .iter()
        .filter(|(ip, entry)| entry.mullvad == Some(false) && snapshot.candidates.contains(ip))
        .map(|(ip, _)| *ip)
        .collect();
    if let Ok(mut state) = authority().write() {
        state.snapshot = Some(snapshot.clone());
        // Admission precedes DNS classification so two starting receivers can
        // answer each other's first discovery handshake. PTR never grants trust.
        state.eligible = snapshot
            .candidates
            .iter()
            .copied()
            .filter(|ip| {
                cache
                    .get(ip)
                    .is_none_or(|entry| entry.mullvad != Some(true))
            })
            .collect();
        state.unknown = cache
            .iter()
            .filter(|(_, entry)| entry.failures > 0)
            .map(|(ip, _)| *ip)
            .collect();
        #[cfg(test)]
        {
            state.classified_count = cache
                .values()
                .filter(|entry| entry.mullvad.is_some())
                .count();
            state.mullvad_count = cache
                .values()
                .filter(|entry| entry.mullvad == Some(true))
                .count();
        }
    }
    tx.send_if_modified(|current| {
        if current.as_ref().ok() == Some(&eligible) {
            false
        } else {
            *current = Ok(eligible);
            true
        }
    });
}

/// Observes Windows route notifications and performs bounded local DNS work.
/// Dropping this future closes sockets, clears route authority and unregisters
/// the Windows callback through net-route's owned Handle. No detached loops.
pub async fn run(cancel: CancellationToken, tx: watch::Sender<Result<HashSet<Ipv4Addr>, String>>) {
    let _clear = ClearOnDrop;
    invalidate();
    loop {
        if cancel.is_cancelled() || tx.is_closed() {
            return;
        }
        let handle = match Handle::new() {
            Ok(handle) => Arc::new(handle),
            Err(_) => {
                let _ = tx.send_replace(Err(
                    "Windows route notifications are unavailable; retrying safely.".to_owned(),
                ));
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tx.closed() => return,
                    _ = tokio::time::sleep(REPAIR) => continue,
                }
            }
        };
        let stream = handle.route_listen_stream();
        tokio::pin!(stream);
        let mut snapshot: Option<Snapshot> = None;
        let mut cache: HashMap<Ipv4Addr, Classification> = HashMap::new();
        let mut pending = FuturesUnordered::new();
        let mut in_flight = HashSet::new();
        let mut refresh_at = Instant::now();
        let mut dirty = true;
        loop {
            let now = Instant::now();
            if !dirty {
                if let Some(current) = &snapshot {
                    let mut due: Vec<_> = cache
                        .iter()
                        .filter(|(ip, entry)| entry.next_query <= now && !in_flight.contains(*ip))
                        .map(|(ip, entry)| (entry.mullvad.is_some(), *ip))
                        .collect();
                    due.sort_unstable();
                    for (_, ip) in due
                        .into_iter()
                        .take(DNS_CONCURRENCY.saturating_sub(pending.len()))
                    {
                        in_flight.insert(ip);
                        pending.push(classify_ptr(ip, current.local_ipv4[0]));
                    }
                }
            }
            let query_at = if dirty || pending.len() >= DNS_CONCURRENCY {
                now + REPAIR
            } else {
                cache
                    .iter()
                    .filter(|(ip, _)| !in_flight.contains(*ip))
                    .map(|(_, entry)| entry.next_query)
                    .min()
                    .unwrap_or(now + REPAIR)
            };
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                _ = tx.closed() => return,
                event = stream.next() => {
                    if event.as_ref().is_some_and(|event| !relevant_event(event, snapshot.as_ref())) { continue; }
                    // Even unrelated events invalidate first: interface/profile
                    // transitions must never retain stale route admission.
                    invalidate();
                    let _ = tx.send_replace(Err("Tailscale routes changed; refreshing discovery.".to_owned()));
                    pending.clear();
                    in_flight.clear();
                    dirty = true;
                    if event.is_none() { break; }
                    // Fixed first-event deadline avoids starvation in a storm.
                    refresh_at = refresh_at.min(Instant::now() + DEBOUNCE);
                }
                _ = tokio::time::sleep_until(refresh_at) => {
                    let result = tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tx.closed() => return,
                        result = read_snapshot(handle.clone()) => result,
                    };
                    refresh_at = Instant::now() + REPAIR;
                    dirty = false;
                    match result {
                        Ok(next) => {
                            if snapshot.as_ref().is_none_or(|old| !old.same_inventory(&next)) {
                                invalidate();
                                pending.clear();
                                in_flight.clear();
                                let same_profile = snapshot.as_ref().is_some_and(|old| {
                                    old.if_index == next.if_index && old.local_ipv4 == next.local_ipv4
                                });
                                if !same_profile { cache.clear(); }
                                cache.retain(|ip, _| next.candidates.contains(ip));
                                for ip in &next.candidates {
                                    cache.entry(*ip).or_insert(Classification {
                                        mullvad: None, next_query: Instant::now(), failures: 0,
                                    });
                                }
                            }
                            publish(&next, &cache, &tx);
                            snapshot = Some(next);
                        }
                        Err(error) => {
                            invalidate();
                            pending.clear();
                            in_flight.clear();
                            cache.clear();
                            snapshot = None;
                            let _ = tx.send_replace(Err(error));
                        }
                    }
                }
                Some((ip, result)) = pending.next(), if !pending.is_empty() => {
                    in_flight.remove(&ip);
                    if let Some(entry) = cache.get_mut(&ip) {
                        entry.mullvad = Some(result.unwrap_or(false));
                        entry.failures = if result.is_some() { 0 } else { entry.failures.saturating_add(1) };
                        let delay = if result.is_some() { CLASSIFICATION_RECHECK } else {
                            Duration::from_secs((30u64 << entry.failures.min(4)).min(300))
                        };
                        entry.next_query = Instant::now() + delay;
                    }
                    if let Some(current) = &snapshot { publish(current, &cache, &tx); }
                }
                _ = tokio::time::sleep_until(query_at) => {}
            }
        }
        invalidate();
        let _ = tx.send_replace(Err(
            "Windows route observer stopped; reconnecting.".to_owned()
        ));
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tx.closed() => return,
            _ = tokio::time::sleep(REPAIR) => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface() -> netdev::Interface {
        let mut interface = netdev::Interface::dummy();
        interface.name = format!("{{{ADAPTER_GUID}}}");
        interface.index = 7;
        interface.flags = netdev::interface::flags::IFF_UP;
        interface.oper_state = netdev::interface::state::OperState::Up;
        interface.ipv4 =
            vec![netdev::ipnet::Ipv4Net::new(Ipv4Addr::new(100, 70, 1, 1), 32).unwrap()];
        interface
    }

    #[test]
    fn exact_routes_only_and_adapter_identity_required() {
        let interface = interface();
        let peer = Ipv4Addr::new(100, 70, 1, 2);
        let routes = vec![
            Route::new(peer.into(), 32).with_ifindex(7),
            Route::new(QUAD100.into(), 32).with_ifindex(7),
            Route::new(Ipv4Addr::new(100, 70, 1, 1).into(), 32).with_ifindex(7),
            Route::new(Ipv4Addr::new(100, 70, 1, 3).into(), 32).with_ifindex(8),
            Route::new(Ipv4Addr::new(100, 71, 0, 0).into(), 24).with_ifindex(7),
        ];
        assert_eq!(
            select_snapshot(std::slice::from_ref(&interface), &routes)
                .unwrap()
                .candidates,
            HashSet::from([peer])
        );
        let mut fake = interface.clone();
        fake.name = "Tailscale".to_owned();
        assert!(select_snapshot(&[fake], &routes).is_err());
        let mut down = interface;
        down.oper_state = netdev::interface::state::OperState::Down;
        assert!(select_snapshot(&[down], &routes).is_err());
    }

    #[test]
    fn aggregated_and_ipv6_only_fail_closed() {
        let mut interface = interface();
        let aggregate = Route::new(Ipv4Addr::new(100, 64, 0, 0).into(), 10).with_ifindex(7);
        assert!(select_snapshot(std::slice::from_ref(&interface), &[aggregate]).is_err());
        interface.ipv4.clear();
        assert!(select_snapshot(&[interface], &[]).is_err());
    }

    #[test]
    fn refreshed_deadline_does_not_change_inventory_identity() {
        let snapshot = select_snapshot(&[interface()], &[]).unwrap();
        let mut refreshed = snapshot.clone();
        refreshed.valid_until += Duration::from_secs(300);
        assert!(snapshot.same_inventory(&refreshed));
        refreshed.candidates.insert(Ipv4Addr::new(100, 70, 1, 2));
        assert!(!snapshot.same_inventory(&refreshed));
        refreshed = snapshot.clone();
        refreshed.if_index += 1;
        assert!(!snapshot.same_inventory(&refreshed));
        refreshed = snapshot.clone();
        refreshed.local_ipv4 = vec![Ipv4Addr::new(100, 70, 1, 3)];
        assert!(!snapshot.same_inventory(&refreshed));
    }

    #[test]
    fn mullvad_suffix_is_label_bound_case_insensitive() {
        assert!(is_mullvad_name("de-fra-wg-001.MULLVAD.ts.net."));
        assert!(!is_mullvad_name("my-mullvad.ts.net"));
        assert!(!is_mullvad_name("x.mullvad.ts.net.evil.test"));
        assert!(!is_mullvad_name("mullvad.ts.net"));
    }

    #[test]
    fn dns_rejects_malformed_packets_and_wrong_transaction() {
        let query = Query::query(
            reverse_name(Ipv4Addr::new(100, 70, 1, 2)).unwrap(),
            RecordType::PTR,
        );
        assert_eq!(parse_ptr_response(&[0xff; 20], 7, &query), None);
        let mut message = Message::new(8, MessageType::Response, OpCode::Query);
        message.add_query(query.clone());
        assert_eq!(
            parse_ptr_response(&message.to_vec().unwrap(), 7, &query),
            None
        );
    }

    #[test]
    fn dns_classifies_only_matching_complete_ptr_answers() {
        use hickory_proto::rr::{rdata::PTR, Record};
        let query = Query::query(
            reverse_name(Ipv4Addr::new(100, 70, 1, 2)).unwrap(),
            RecordType::PTR,
        );
        let mut message = Message::new(7, MessageType::Response, OpCode::Query);
        message.add_query(query.clone());
        message.add_answer(Record::from_rdata(
            query.name().clone(),
            5,
            RData::PTR(PTR(Name::from_ascii("de-fra-001.mullvad.ts.net.").unwrap())),
        ));
        assert_eq!(
            parse_ptr_response(&message.to_vec().unwrap(), 7, &query),
            Some(true)
        );
        message.answers[0].data =
            RData::PTR(PTR(Name::from_ascii("friend.example.ts.net.").unwrap()));
        assert_eq!(
            parse_ptr_response(&message.to_vec().unwrap(), 7, &query),
            Some(false)
        );
        message.metadata.truncation = true;
        assert_eq!(
            parse_ptr_response(&message.to_vec().unwrap(), 7, &query),
            None
        );
        message.metadata.truncation = false;
        message.answers[0].name = Name::from_ascii("wrong.in-addr.arpa.").unwrap();
        assert_eq!(
            parse_ptr_response(&message.to_vec().unwrap(), 7, &query),
            None
        );
    }

    #[tokio::test]
    #[ignore = "Opt-in passive Windows IP Helper inspection; no DNS, peer probes, CLI or LocalAPI"]
    async fn passive_windows_inventory() {
        let handle = Arc::new(Handle::new().unwrap());
        let snapshot = read_snapshot(handle).await.unwrap();
        assert!(!snapshot.local_ipv4.is_empty());
        assert!(snapshot.candidates.len() <= MAX_CANDIDATES);
        eprintln!(
            "Passive Tailscale inventory: {} bounded IPv4 candidates",
            snapshot.candidates.len()
        );
    }

    #[tokio::test]
    #[ignore = "Opt-in at most three device-local quad100 PTR queries; no remote peer connections"]
    async fn bounded_windows_quad100_classification() {
        let snapshot = read_snapshot(Arc::new(Handle::new().unwrap()))
            .await
            .unwrap();
        let mut addresses: Vec<_> = snapshot.candidates.iter().copied().collect();
        addresses.sort_unstable();
        assert!(!addresses.is_empty(), "No peer routes to classify");
        let mut answers = 0;
        let mut mullvad = 0;
        for ip in addresses.into_iter().take(3) {
            let (_, classification) = classify_ptr(ip, snapshot.local_ipv4[0]).await;
            answers += usize::from(classification.is_some());
            mullvad += usize::from(classification == Some(true));
        }
        eprintln!(
            "Bounded quad100 classification: {answers} answers; {mullvad} Mullvad classifications"
        );
        assert!(answers > 0, "Quad100 returned no valid matching PTR answer");
    }

    #[tokio::test]
    #[ignore = "Opt-in full provider lifecycle: at most15s device-local quad100 PTR; no peer TCP"]
    async fn bounded_windows_provider_lifecycle() {
        let cancel = CancellationToken::new();
        let (tx, _rx) = watch::channel(Err("starting test".to_owned()));
        let started = Instant::now();
        let worker = tokio::spawn(run(cancel.clone(), tx));
        let completed = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                {
                    let state = authority().read().unwrap();
                    if let Some(snapshot) = &state.snapshot {
                        if state.classified_count == snapshot.candidates.len() {
                            return (
                                snapshot.candidates.len(),
                                state.mullvad_count,
                                state.unknown.len(),
                            );
                        }
                    }
                }
                // Test-only completion observation; production has no polling
                // observer. Reported elapsed time includes this10ms quantization.
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        let elapsed = started.elapsed();
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("Provider did not cancel promptly")
            .expect("Provider panicked");
        assert!(authority().read().unwrap().snapshot.is_none());
        let (candidates, mullvad, unknown) =
            completed.expect("Local PTR classification did not complete within15s");
        eprintln!("Provider lifecycle: candidates={candidates}, Mullvad={mullvad}, nonservice={}, unknown={unknown}, elapsed_ms={}, cancellation=clean",
            candidates.saturating_sub(mullvad + unknown), elapsed.as_millis());
    }
}
