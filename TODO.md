# Performance & Architecture TODO

> This is a Rust + Iced rewrite of ServerPickerX. Some patterns were carried over from the original repo and may not be the optimal solution for Rust/async/Iced. The items below are candidates for incremental improvement.

## Legend

- `P0` — Do first: localized, high impact, low risk
- `P1` — High impact, slightly more involved
- `P2` — Worth doing after P0/P1, or situational

---

## P0 — Quick, High-Impact Wins

### 1. Bound ping concurrency in `Message::PingAll` / `PingPollTick` ✅
**File:** `src/app.rs`

Implemented via a `tokio::sync::Semaphore` in `Message::PingAll` and `Message::PingPollTick`. Each server still pings independently, but at most `MAX_CONCURRENT_PINGS = 16` probes are active at once. Results still stream back one-by-one via `Message::ServerPinged`.

---

### 2. Reuse a single `surge_ping::Client` across all pings ✅
**File:** `src/services/ping_service.rs`

Implemented with a lazily-initialized `Arc<Client>` stored in `PingService`. The client is created on first ping rather than at construction, so service creation no longer requires a Tokio runtime. Each pinger now gets a unique `PingIdentifier` from an atomic counter instead of the hard-coded `PingIdentifier(0)`.

---

### 3. Cache the filtered server list ✅
**File:** `src/app.rs`

Implemented with `cached_filtered_indices: Vec<usize>` in `ServerPickerApp`. The filter/sort work is done once per input change and subsequent `filtered_servers()` calls just map indices to references. Invalidated on:
- `SearchChanged`
- `ToggleGroupByRegion`
- `ServersLoaded`

`ServerPinged` mutates `server_data.servers` in place, so cached indices remain valid.

---

### 4. Debounce `SettingsService::save()`
**File:** `src/services/settings_service.rs`

Pretty-printed JSON is written to disk on almost every UI interaction (`GameModeChanged`, `ToggleCluster`, preset renames, etc.). Add a debounced/queued save task (e.g. 500 ms cooldown, coalesce redundant writes) to avoid fsync storms.

---

## P1 — Bigger Architecture Improvements

### 5. Virtualize the server table
**File:** `src/ui/server_table.rs`

Every filtered row is materialized on every `view()` call:

```rust
let rows: Column<'a, Message> = app.filtered_servers().into_iter().enumerate().fold(...)
```

Iced has no built-in virtual list. Options:
- Use `oriel-iced` (virtualized data grid).
- Write a custom lazy list that renders only the visible viewport + overscan.

This is the single biggest UI scaling win as server counts grow.

---

### 6. Remove string clones from hot view code ✅
**File:** `src/ui/server_table.rs`

Done as part of the region-grouping UI refactor. Descriptions, names, and region headers now borrow `&str` from `Server` where possible instead of cloning per row.

---

### 7. Make relay scanning within one server concurrent ✅
**File:** `src/services/ping_service.rs`

Implemented alongside #2. Relays are now scanned concurrently with `futures::stream::iter(...).buffer_unordered(MAX_CONCURRENT_RELAY_SCANS)` (default limit of 4), so a server with many relays does not scan them one-by-one.

---

### 8. Optimize keyword filtering
**File:** `src/core/filter.rs`

`is_server_accepted` and `find_cluster_keyword` call `description.to_lowercase()` once per keyword check. For large keyword lists:
- Pre-lowercase keywords at load time.
- Consider tokenizing descriptions into a `HashSet` of lowercase words and checking set membership for `Include`/`Exclude`.

---

### 9. Cache Valve SDR config by revision
**File:** `src/core/valve_api.rs`, `src/app.rs`

`load_server_data` always fetches and parses the full JSON. Store the last `revision` per `app_id` and skip re-parsing if unchanged. Even a simple “fetch → compare revision → skip if same” saves serde work on startup.

---

## P2 — Polish / Situational

### 10. Reduce firewall script size for iptables
**File:** `src/services/firewall/linux.rs`

`block_iptables` emits one `iptables -A ... -j DROP` per IP. Options:
- Prefer `nftables` set path (already done when available).
- For iptables, use `ipset` (one set, one rule).
- At minimum, deduplicate IPs before generating the script.

---

### 11. Use compact JSON for settings
**File:** `src/services/settings_service.rs`

`serde_json::to_string_pretty` is slower and larger than `to_string`. The file is not hand-edited.

---

### 12. Embed decoded flag SVGs at build time
**File:** `src/ui/server_table.rs`

`FLAG_CACHE` decodes base64 SVGs at first access. Use `include_bytes!` / build-time generation to remove startup decode latency.

---

### 13. Tune release profile
**File:** `Cargo.toml`

Add a release profile for better binary size and startup:

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
```

---

### 14. Skip auto-ping when window is not focused
**File:** `src/app.rs`

`subscription` already gates on `ping_polling_enabled`. Add a window-focus check (Iced exposes `window::Event`) to stop probes when the app is minimized/backgrounded.

---

### 15. Replace `futures::future::join_all` in `ping_servers`
**File:** `src/services/ping_service.rs`

`join_all` is fine, but a `buffer_unordered` stream gives the same concurrency control and cancellation behavior as the app-level fix.

---

## Suggested Order

1. ~~P0 #1 — Bound ping concurrency~~ ✅
2. ~~P0 #2 — Reuse shared `surge_ping::Client`~~ ✅
3. ~~P1 #7 — Concurrent relay scanning~~ ✅
4. ~~Replace keyword clustering with UI region grouping~~ ✅
5. ~~P1 #6 — Remove view-layer string clones~~ ✅
6. ~~P0 #3 — Cache filtered server list~~ ✅
7. P0 #4 — Debounce settings saves
8. P1 #5 — Virtualize server table
9. P1 #8 — Optimize keyword filtering
10. P1 #9 — Valve revision cache
11. P2 items as time allows

---

## Notes for Rewrite Context

- The original repo’s pattern of "spawn everything at once" does not map cleanly to Rust’s async runtime; bounded concurrency is usually faster and stabler.
- Iced’s immediate-mode view functions are not free: every allocation in `view()` is paid per frame.
- `surge-ping` is not a "one client per target" library; its performance comes from the shared socket + waiter model.
