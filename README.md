# server-picker-rs

Rust + Iced rewrite of **ServerPickerX**, a desktop app for blocking/unblocking
game servers based on their geographic location.

## Status

Phase 0/1 complete:
- Project bootstrapped with Cargo.
- Core data models ported from the C# app (`GameModeDefinition`, `Server`,
  `Relay`, `ServerData`).
- Keyword include/exclude/none filtering and cluster merging implemented.
- Valve SDR config fetch + parse implemented with a mockable HTTP client.
- Unit tests for parsing, filtering, and clustering.

Phase 2 complete:
- `AppSettings` plain data model (game mode, language, cluster toggle,
  version-check toggle, per-app revisions, last-selected presets, presets).
- `SettingsService` with JSON load/save via `directories::ProjectDirs`.
- Preset add/update/remove/duplicate detection/prune operations.
- Additional unit tests for settings and preset operations.

Phase 3 complete:
- `FirewallService` trait + `CommandRunner` abstraction for testability.
- Linux backend with `nftables` (preferred, set-based blocking) and
  `iptables` fallback (custom chain).
- `block_servers`, `unblock_servers`, `reset_firewall` for both backends.
- Privilege escalation per command via `pkexec`/`sudo` (uses the desktop
  polkit authentication agent instead of relaunching the whole app).
- Windows firewall placeholder for Phase 10.
- Firewall command builder unit tests.

Phase 4 complete:
- `PingService` with ICMP via `surge-ping` and TCP-connect fallback.
- Concurrent ping of all visible servers using `futures::join_all`.
- Lowest-RTT relay selection + 4 follow-up probes for packet loss.
- `Relay` now carries `port_range_min`/`port_range_max` parsed from Valve data.
- `ServerStatus` extended with `Pinging`/`Probing` states.

Phase 5 complete:
- Iced application shell in `src/app.rs` using the `iced::application` builder.
- `main_view.rs` with game-mode picker, search box, cluster toggle,
  ping/block/unblock buttons, progress bar, and footer links.
- `server_table.rs` custom table with checkboxes, selection, and status symbols.
- Async wiring: load servers, ping all, block/unblock via `Task::perform`.
- Privilege escalation per firewall operation via `pkexec`/`sudo` (polkit
  authentication agent) instead of relaunching the app.

## Quick Start

```bash
cargo test
cargo run
```

## License

MIT
