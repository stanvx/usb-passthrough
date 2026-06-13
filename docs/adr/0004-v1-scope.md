# v1.0 scope: reliability first, performance + platforms, ecosystem last

The v1.0 release includes the three reliability primitives (structured errors, hot-plug detection, auto-reconnect), all M11 performance optimizations, macOS server + client, Docker multi-arch deployment, a full web UI with REST API, multiple simultaneous client connections, a latency dashboard, Linux client daemon, and RetroPie/Lakka/Steam Link ecosystem packaging. Isochronous transfers, session persistence, USB 3.0 SuperSpeed, IPv6, Prometheus metrics, bandwidth throttling, Home Assistant add-on, iOS companion app, and custom embedded firmware are explicitly deferred.

The decision was shaped by a competitive review of VirtualHere (the dominant commercial USB/IP product), which revealed four gaps in the current codebase: Linux client daemon (no background service mode on Linux), embedded server distribution (VirtualHere CloudHub for Raspberry Pi), REST API / programmatic control, and IPv6. The first three were promoted to v1.0; IPv6 was deferred because LAN gaming is IPv4-only and adding dual-stack later is non-invasive.

## Considered Options

- **Minimal v1.0 (reliability only):** structured errors + hot-plug + auto-reconnect, defer everything else. Rejected — ships a "reliable" product nobody can easily install (no client daemon, no ecosystem packaging) and leaves VirtualHere's core features (macOS, web UI, Docker) unaddressed.
- **Maximal v1.0 (everything in M11-M14):** rejected — isochronous is a quarter-scale protocol expansion (ADR-0002), session persistence requires stable serialised state on both sides, and custom embedded firmware (Buildroot/Yocto) is a maintenance commitment disproportionate to v1.0 value.
- **Competitive v1.0 (chosen):** reliability primitives + performance + macOS + Docker + web UI/REST + ecosystem packaging + client daemon. Covers the VirtualHere gaps that matter for the target audience (gamers, prosumers, home users) while respecting the architectural constraints of ADR-0002 and ADR-0003.

## Ordering

Implementation must follow the dependency chain established in ADR-0003: structured errors first (everything else depends on categorising failures), then hot-plug, then auto-reconnect. Performance work and platform expansion (macOS, Docker) can run in parallel with reliability. Ecosystem packaging comes last — it depends on the client daemon existing and the system being stable.

## Consequences

- ROADMAP.md milestones M11-M14 are restructured to reflect the v1.0 scope.
- macOS I/O Kit enumeration (server) and VHCI equivalent (client) are new platform targets requiring significant implementation.
- The web UI + REST API introduces a new codebase component not previously in the architecture.
- Isochronous transfers remain a known gap — documentation must be honest that audio devices and webcams do not work in v1.0.
