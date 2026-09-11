import type { DiscoveredDevice } from "$lib/state/model";

export function peerRouteLabel(device: DiscoveredDevice): string {
  if (!device.online) return "Offline";
  if (device.network === "tailscale") return "Tailscale";
  if (device.network === "lan") return "LAN";
  return "Online";
}

export function peerRouteDescription(device: DiscoveredDevice): string {
  const route = peerRouteLabel(device);
  if (!device.online) return route;
  if (device.lanIp && device.tailscaleIp) {
    return device.network === "tailscale"
      ? "Tailscale · LAN also discovered"
      : "LAN · Tailscale available as fallback";
  }
  return route;
}
