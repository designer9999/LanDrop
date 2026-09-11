# Local Windows network diagnostics

Run this capture yourself while the connection problem is occurring. It works in Windows PowerShell 5.1 without administrator access. It does not open or control LanDrop, Tailscale, Mullvad, or another VPN.

From the repository directory in Windows PowerShell:

```powershell
powershell.exe -NoProfile -File .\scripts\Collect-LanDropNetworkDiagnostics.ps1
```

The default capture lasts up to 120 seconds, plus a small allowance to stop its own timed-out diagnostic helpers and save the report. A short smoke capture is available:

```powershell
powershell.exe -NoProfile -File .\scripts\Collect-LanDropNetworkDiagnostics.ps1 -DurationSeconds 10
```

If Windows blocks the script under your execution policy, the capture has not started. Do not change machine policy just to collect a report; ask your administrator how to run the reviewed local script.

## What is recorded

- Process names and PIDs for `landrop`, `tailscaled`, and `tailscale-ipn` only. No command lines or process contents.
- Adapter names, descriptions, status, link speed, IP interface status and metrics, default routes (including IPv4/IPv6 split-default `/1` routes used by some VPNs), and configured DNS servers.
- Timestamped DNS resolution and HTTPS HEAD checks for `github.com` and `tailscale.com`. HTTPS response bodies are discarded; certificate verification stays enabled.
- Individual failures, timeout statuses, HTTP status codes, and curl exit codes. Raw error output is excluded.

Snapshots and probes repeat approximately every 10 seconds. Slow checks consume their own timeout budget, so fewer samples may fit in a capture. A 10-second capture can end before every probe completes; these entries are marked `deadline_reached`. Each probe has a timeout and runs independently. An unavailable cmdlet or failed internet connection does not prevent the remaining results from being saved.

The script never invokes the Tailscale CLI or LocalAPI, including `tailscale status`. It does not read Tailscale logs or profiles, open VPN windows, capture packets, upload results, change DNS/firewall/VPN settings, or stop existing applications. Only helper processes created by the capture itself may be terminated when their diagnostic timeout expires. There are no external power-management or connectivity controls.

## Where the report goes

Reports are timestamped JSON files in:

```text
%LOCALAPPDATA%\LanDrop\Diagnostics\network-<timestamp>-<id>.json
```

This is a diagnostics folder, not LanDrop's application-state store. The script prints the exact report path. To choose another local folder:

```powershell
powershell.exe -NoProfile -File .\scripts\Collect-LanDropNetworkDiagnostics.ps1 -OutputDirectory 'D:\Diagnostics\LanDrop'
```

The destination must be writable. No installer or third-party package is required. If `curl.exe` is unavailable, HTTPS checks are marked `curl_unavailable` and other checks continue. Internet failures cannot block writing an otherwise writable local destination.

The script creates the report at startup and atomically refreshes it after each sample. `completionStatus` is `in_progress` until the final save marks it `complete` or `failed`. If the capture is interrupted, the last saved partial report remains; the current unfinished sample may be missing. A temporary file with the same name plus `.tmp` may remain if writing was interrupted. Leave the capture running until it prints the saved path for a completed report.

## Reading and sharing a report

`http_response` means the HTTPS endpoint responded; inspect `httpStatus` separately for website errors. `resolved` means the DNS check returned target website addresses. `failed`, `timeout`, and `unavailable` distinguish unsuccessful probes from unavailable local collectors. A helper timeout can also include PowerShell/module startup overhead, especially in the short smoke capture; it does not prove that a VPN caused the failure.

Compare timestamps, interface state, route changes, DNS configuration, and the two independent website probes. These observations cannot prove which application changed a network setting or whether Tailscale was connected to a particular profile. The script deliberately avoids the Tailscale interfaces that could disturb the issue under investigation.

The JSON can reveal interface names, private gateways, DNS server addresses, and which of the three named processes were running. Review it before sharing it manually. No credentials, chat content, files, raw logs, full process lists, public-IP lookup result, or packet contents are collected. Ordinary requests to the two websites still disclose the connection's source address to those websites, as normal internet traffic does. Reports remain local until you choose to share or delete them.

## Local verification

On September 11, 2026, Windows PowerShell 5.1 parsing and a 10-second normal
capture passed with valid JSON, all five snapshot categories, and both DNS/HTTPS
targets. A second capture used an unreachable proxy only within the test process
to simulate HTTPS failure: both failures were recorded and the local report still
completed successfully. No Windows proxy setting or VPN configuration was changed.
