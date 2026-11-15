# ServerPickerX Rust Rewrite — Handoff

**Date:** 2026-11-15  
**Project:** `/home/danko/repo/server-picker-rs`  
**Plan:** `/home/danko/.opencode/plan/serverpickerx-rewrite-plan.md`

---

## Current Status

Phases 0–7 of the rewrite plan are complete. The application is a runnable Iced desktop app with a custom borderless title bar, loads game definitions, fetches Valve SDR config, displays servers in a custom table with SVG flags, tracks blocked/unblocked state per session, and can save/apply presets via a modal preset manager.

---

## Completed Work

| Phase | Summary |
|-------|---------|
| 0/1 | Project bootstrapped; core data models (`GameModeDefinition`, `Server`, `Relay`, `ServerData`); Valve API fetch/parse with mockable client; keyword filtering and cluster merging; unit tests. |
| 2 | `AppSettings` plain-data model; `SettingsService` with JSON persistence via `directories::ProjectDirs`; preset add/update/remove/duplicate detection/prune. |
| 3 | `FirewallService` trait; Linux `nftables` (set-based) and `iptables` (custom chain) backends; Windows placeholder; per-command `pkexec`/`sudo` privilege escalation. |
| 4 | `PingService` with ICMP via `surge-ping` and TCP-connect fallback; concurrent pings; lowest-RTT relay + 4 probes for packet loss; `port_range` parsed from Valve data. |
| 5 | Iced application shell (`app.rs`); main view with game-mode picker, search, cluster toggle, ping/block/unblock buttons, progress bar, footer links; custom selectable server table. |
| 6 | Custom borderless title bar with drag region, minimize, maximize/restore, and close buttons; window controls use the actual window ID fetched at startup. |
| 7 | Preset manager UI: preset dropdown, Apply/Save/Manage controls, modal dialog for create/rename/duplicate/delete/apply; presets store game mode, cluster mode, search text, and blocked server keys; applying a preset resets the firewall then blocks only the saved servers so UI and firewall stay consistent. |

---

## How to Run

```bash
cd /home/danko/repo/server-picker-rs
cargo run
```

Requirements:
- Linux desktop with X11 or Wayland (for the GUI).
- `nftables` or `iptables` installed for firewall operations.
- `pkexec` (preferred) or `sudo` for privilege escalation when blocking/unblocking.
- Internet to fetch Valve SDR config.

---

## Verification

```bash
cargo fmt --check   # clean
cargo test          # 40 passed
cargo build         # succeeds
```

---

## Architecture Notes

- **No global mutable state.** Services are created in `AppFlags::new()` and passed into the Iced app.
- **Services behind traits.** `FirewallService`, `ValveApiClient`, and `CommandRunner` are trait-based for testability.
- **Settings as plain data.** `AppSettings` holds data; `SettingsService` owns persistence behavior.
- **Iced 0.13 builder API.** `src/main.rs` uses `iced::application(...).theme(...).run_with(...)`.

---

## Known Limitations / Next Steps

- **Flags:** ✅ Implemented as SVG flag icons via the `rs-grid-icons` crate (254 embedded flags, sourced from MIT-licensed `flag-icons`). Country codes are derived from server descriptions; unknown regions fall back to a `??` code badge.
- **Ping polling:** ✅ Active rolling ping polling every 15 seconds. Each server keeps the last 10 ping samples; the Ping column shows the rolling average. Toggle via footer "Auto-Ping: On/Off" button.
- **Firewall privilege escalation:** ✅ Linux firewall commands are now batched into a single `pkexec`/`sudo` shell script per operation, so polkit only prompts once per Block/Unblock/Reset instead of once per server/rule.
- **State column / firewall status:** ✅ Replaced the broken emoji Status column with a text-based State column showing "Blocked" / "Open" based on in-session block/unblock actions. The state is reset when switching game modes or cluster views because server keys change. A TODO is in place to query the real firewall state in a future polish pass.
- **Phase 7 — Preset manager:** ✅ Full UI for create/rename/duplicate/delete/apply presets. Applying a preset resets the firewall and re-blocks only the saved keys, keeping the UI and firewall consistent.
- **Phase 8 — Revision sync:** Startup revision comparison, stale-entry pruning, and warning dialog not implemented.
- **Phase 9 — Settings window & i18n:** Settings button in footer is a placeholder. Localization files not ported.
- **Phase 10 — Windows port:** `firewall/windows.rs` is a stub.
- **Phase 11 — Polish:** Error dialogs, logging to file, firewall-state query API, and remaining unit tests.

---

## Important Implementation Details

- Firewall backend detection uses `which nft` / `which iptables` instead of running the tools, so it works for non-root users.
- When not root, firewall commands are wrapped with `pkexec` (preferred) or `sudo`, triggering the desktop polkit auth agent per operation.
- `revision` from Valve is parsed as `serde_json::Value` because it can be an integer or a string.
- `Relay` carries `port_range_min`/`port_range_max` parsed from the Valve `port_range` array; TCP fallback uses the minimum port.

---

## File Map

```
server-picker-rs/
├── Cargo.toml
├── assets/ServerDefinitions.json
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── app.rs
│   ├── core/
│   │   ├── mod.rs
│   │   ├── game_mode.rs
│   │   ├── server.rs
│   │   ├── filter.rs
│   │   ├── preset.rs
│   │   ├── settings.rs
│   │   └── valve_api.rs
│   ├── services/
│   │   ├── mod.rs
│   │   ├── firewall/
│   │   │   ├── mod.rs
│   │   │   ├── linux.rs
│   │   │   └── windows.rs
│   │   ├── ping_service.rs
│   │   └── settings_service.rs
│   └── ui/
│       ├── mod.rs
│       ├── main_view.rs
│       ├── preset_dialog.rs
│       ├── server_table.rs
│       └── title_bar.rs
```

---

## Recommended Next Action

Continue with **Phase 8** (revision sync): add startup revision comparison against the stored `server_revisions`, prune stale preset entries when Valve removes servers, and show a warning dialog when entries were pruned.
