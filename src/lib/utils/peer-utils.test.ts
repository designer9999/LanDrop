import { describe, expect, it } from "vitest";
import { peerRouteDescription, peerRouteLabel } from "./peer-utils";
import type { DiscoveredDevice } from "$lib/state/model";

const device: DiscoveredDevice = {
  id: "peer",
  alias: "Peer",
  deviceType: "desktop",
  ip: "192.168.1.2",
  online: true,
  color: 0,
};

describe("peer route labels", () => {
  it("describes the backend's selected route and fallback without inferring a route from IP", () => {
    expect(peerRouteLabel(device)).toBe("Online");
    expect(peerRouteLabel({ ...device, network: "tailscale" })).toBe("Tailscale");
    expect(
      peerRouteDescription({
        ...device,
        network: "lan",
        lanIp: device.ip,
        tailscaleIp: "100.64.0.2",
      }),
    ).toBe("LAN · Tailscale available as fallback");
    expect(peerRouteDescription({ ...device, online: false, network: "tailscale" })).toBe(
      "Offline",
    );
  });
});
