# Security Policy

## Project Status

usb-passthrough is alpha software, pre-1.0. The transport, crypto,
and platform integrations are under active development. Treat it as
an early-adopter build: useful, not production-hardened.

## Supported Versions

Only the latest commit on `main` is supported. No tagged releases,
no LTS branch, no backports. If a fix lands, it ships on `main`;
pull a fresh build. When 1.0 ships, this section will need
updating to define which versions receive security patches and for
how long.

## Reporting a Vulnerability

The preferred channel is GitHub Security Advisories. The private
advisory form lets you share details, PoC code, and a working draft
fix with the maintainer without disclosing the issue publicly.

- **Primary:** [Open a private security advisory](https://github.com/stanvx/usb-passthrough/security/advisories/new)
- **Fallback (low-sensitivity only):** open a public GitHub issue
  tagged `security`. Use this only when public discussion does not
  materially help an attacker — for example, a hardening suggestion
  or a defence-in-depth check with no exploitable path.

For high-sensitivity issues (crypto flaws, RCE, auth bypass,
anything in the in-scope list below that would be meaningfully worse
if disclosed early), always prefer the private channel. Do not post
PoC code in a public issue, even with a `security` tag.

## What Is In Scope

Paths below are current at time of writing; if a subsystem moves,
the description takes precedence.

### Cryptography (`shared/usbip-core/src/crypto.rs`)

The project ships its own X25519 field arithmetic and Montgomery
ladder in pure Rust, alongside `ring`-backed AES-256-GCM and
HKDF-SHA256. In scope:

- Bugs in the hand-rolled X25519: constant-time violations,
  incorrect clamping, field reduction errors, non-CT branches in
  `sub` / `inv`.
- Nonce reuse in `encrypt` / `encrypt_usbip_message`. Both pull
  from `SystemRandom` today; a future move to counter-derived
  nonces or shared RNG state is a finding.
- AAD binding in `encrypt_usbip_message`: the 8-byte USB/IP header
  is the associated data, so header-bit tampering must be
  detected. A decrypt path that falls back to `Aad::empty()` is a
  finding.
- HKDF domain separation. `HKDF_INFO`
  (`"usb-passthrough-session-key-v1"`) is the only barrier
  against cross-protocol key reuse. Changing it, or accepting
  attacker-controlled `info`, is a finding.
- `hex_encode` uses `unsafe { String::from_utf8_unchecked(...) }`.
  A `from_hex` regression that emits non-ASCII bytes would be a
  soundness bug.

### Protocol parsing (`shared/usbip-core/src/protocol.rs`, `shared/usbip-core/src/urb.rs`)

All wire structs are `#[repr(C, packed)]` and parsed with
`zerocopy::FromBytes`. In scope:

- Parsing `UsbIpCmdSubmit` / `UsbIpRetSubmit` from untrusted input
  without checking `transfer_buffer_length` / `actual_length`
  against the available payload — a huge declared length with a
  tiny payload could cause OOB reads in callers.
- `UsbIpCmdSubmit::is_control()` checks the raw 8-byte `setup`
  field for non-zero. A URB with the high bit of `devid` / `ep`
  set, or with both `URB_DIR_IN` and OUT direction bits set, is
  malformed and should be rejected, not silently coerced.
- `UsbIpMessage::as_cmd_submit` / `as_ret_submit` / `as_ret_unlink`
  do not validate that the remaining payload matches the struct's
  full size — a 4-byte tail after a 48-byte header would still
  parse.
- `UsbIpDeviceEntry::path_str` / `busid_str` use `unwrap_or("???")`
  on `from_utf8`. Surfacing non-UTF-8 identifiers to a UI is a
  low-severity finding.

### Transport (TCP)

- No server-identity authentication today. A client that knows a
  server's address and the AES-256-GCM session key can connect,
  but cannot confirm it is talking to the expected server (no
  pinned public key, no certificate). Known gap; in scope as a
  threat-model / documentation finding.
- Plaintext fallback when crypto is disabled is a known gap.
- TCP framing: partial reads, partial writes, reordering. A
  48-byte URB header that arrives in two TCP segments must not be
  interpreted as two messages.

### mDNS discovery (`server/usbip-server/src/discovery.rs`, `client/usbip-client/src/discovery.rs`)

- mDNS is LAN-local and unauthenticated. A malicious host can
  advertise a fake `_usbip._tcp.local.` and steer clients to it.
  The client's `browse()` accepts the first `ServiceResolved`
  with no trust anchor.
- The `version` TXT record is hard-coded to `"1.1.1"` in
  `MdnsAdvertiser::start`. Not a security boundary, but a client
  branching on it for security decisions is a finding.
- mDNS responses are not validated against any hostname or subnet
  allowlist. Relying on it for access control is not OK.

### Windows service privilege model

The Windows service runs with administrative privileges (required
to talk to USB drivers); the attack surface is whatever the
service can reach as `SYSTEM`. In scope: local privilege
escalation via named-pipe, RPC, or configuration interfaces; any
path where a non-admin local user can influence which USB device
is exported, the crypto key material, or the bind port; and
insecure service-installer defaults (world-readable config,
predictable pipe names without ACLs, etc.).

### Android USB Host API permission model

The Android app uses the USB Host API, which requires per-device
user consent at attach time. In scope: exported activities or
providers that bypass the consent dialog; persisting or caching
device permission grants in a way that survives an uninstall or
profile change; calling `UsbManager.openDevice` from a
non-foreground context (e.g. a background service) without
re-checking the permission; and JNI bridge paths that let a
Java/Kotlin caller supply a `ByteBuffer` with attacker-controlled
length, or free a buffer the JNI caller still holds.

### URB forwarding edge cases

- **URB smuggling:** a crafted `USBIP_CMD_SUBMIT` with
  `transfer_buffer_length` different from the data actually sent
  could cause the server to over-read (panic / OOB) or
  under-read (stale buffer reuse) when forwarding to the local
  USB stack.
- **Partial-write handling:** a short `write` on the data
  portion of an OUT URB could be misclassified as a full URB.
- **Isochronous `number_of_packets`:** declared but per-packet
  offsets/lengths are not validated against the actual payload —
  a malformed isoc URB could overrun the host kernel buffer.
- **Sequence number reuse:** URBs with the same `devid` and
  `seqnum` from different clients could collide. Per-device model
  makes this low-severity, but worth noting.

### Cross-platform Rust core

Any memory-safety issue in the Rust core is in scope. Pay
particular attention to: the protocol parser (callers of
`ref_from_prefix` may not be safe even though `zerocopy` is); the
hand-rolled X25519 in `crypto.rs`; every `unsafe` block in the
repo; and FFI boundaries (Windows service FFI, Android JNI, any
future C library bindings).

## What Is Out of Scope

- **Physical access to the server machine.** A user with
  root / administrator / physical access to the host running
  `usbip-server` is assumed trusted. Local privilege escalation
  across the user/admin boundary is in scope; everything above
  that is not.
- **Denial of service on the LAN.** Resource exhaustion, connection
  floods, mDNS storms, and similar LAN-local DoS are not in scope.
  The threat model assumes a cooperative LAN.
- **Malicious USB devices.** A peripheral that misbehaves, fails
  enumeration, or carries hostile firmware is the user's problem,
  not the bridge's.
- **Vulnerabilities in upstream dependencies** (`ring`, `mdns_sd`,
  `zerocopy`, `tokio`, etc.) — report those upstream.
- **Theoretical weaknesses with no plausible exploit.**

## Response Timeline

Single-maintainer alpha project. **Acknowledgement:** typically a
few days — the maintainer works on this in their own time.
**Triage:** critical issues (RCE, auth bypass, crypto break) jump
the queue. **Fix:** no guaranteed timeline — critical in days to
a couple of weeks, medium waits for the next release cycle, low
may sit indefinitely. **Pre-1.0 reality:** there are no patch
releases; the fix lands on `main` and you rebuild. The fastest
path to a fix is a small, well-written patch alongside the report.

## Disclosure Policy

Coordinated disclosure. Default window is **90 days** from the
date the maintainer acknowledges the report, extendable by mutual
agreement or shortenable if already public or actively exploited.
The published advisory includes a GHSA-requested CVE, credit to
the reporter in the fix commit and release notes (anonymity on
request), and a brief technical write-up. If the 90-day window
closes without a fix, the reporter is free to disclose publicly;
in practice, please give a few extra days' notice.

## Acknowledgements

No reports received yet. When the first report lands, this section
will list the reporters who have helped improve the security of
the project (with their permission, and with the handle or name
they preferred).
