# Final Whole-Branch Review — device → thing DB rename

**Branch:** `refactor/thing-db-rename` (092a4bcc..4104c45e, 8 commits)
**Reviewer role:** broad pass (cross-cutting defects, consistency breaks, seams task-scoped reviews could not see)
**Date:** 2026-08-26

## Overall verdict: CLEAN TO MERGE

No Critical or Important findings. 5 Minor findings (all ship-as-is, mostly PR-2 tracking items). All ledger deferred minors triaged to ship-as-is. Migration safety, wire-contract preservation, and cross-layer field consistency verified by targeted inspection (gates already green per task reports: workspace 1508/0, clippy 0).

---

## What was verified (broad-pass coverage)

### (a) Cross-layer consistency — 10 end-to-end chains sampled

| Chain | Path | Result |
|---|---|---|
| Thing CRUD/search | `db::thing` rows (`category`, `thing_id`) → `core::models::device::Device` (snake_case `category`) → cloud DTOs (camelCase `category`/`thingId`) | consistent |
| Thing search criteria | `DeviceCriteria.device_type` (Rust name kept for PR-2) → SQL `AND category = ?` (crates/db/src/thing.rs:1678-1679) | correct mapping |
| Alarms | `db::alarm` `thing_id` → `alarm/dto.rs` `thing_id` → camelCase `thingId` | consistent |
| Jobs | core `job.rs` `target_thing_id` (snake JSON) → `cron_executors.rs` reads config key `thing_id` | consistent; breaking documented in CHANGELOG |
| Events | `event/handler/query.rs`/`overview.rs` `thing_id` → `EventSource::thing_id()`; `source_type='device'` literals deliberately preserved (event-source semantics, per plan) | consistent |
| Batch commands | items `thing_id`; request field `device_ids` retained (see M4) | consistent per declared scope |
| Tenant quota | `db::tenant` `thing_limit` → `tenant/service.rs` `plan.thing_limit` | consistent |
| MCP tools | advertised schemas keep `deviceId`/`targetDeviceId` primary + `thingId`/`targetThingId` alias (`mcp/tools/device.rs:32`, `job.rs:33`, `alarm_mcp.rs:50`) | matches branch purpose |
| Agent-host tools | advertise `thingId`; `pool_adapter.rs` audit lookup chain accepts `thing_id`→`thingId`→`deviceId`→`device_id` with dedicated tests (fix commit 965b10fe) | consistent |
| LLM heartbeat contract | prompt still specifies `device_id`; parser `report.rs:72,95` reads `device_id`; `workspace_heartbeat.rs` reads `deviceId` | wire contract unchanged as intended |
| Driver wire | `device_command.rs:11`/`device_property.rs:11` keep Rust field `device_id` with `#[sqlx(rename = "thing_id")]`; MQTT/plugin payloads untouched | as planned |
| Open API | `admin/open/mod.rs` emits `thing_id`/`category` — REST, covered by declared breaking | consistent |

Zero residual old names in SQL contexts outside driver-wire files (grep: `FROM devices`/`device_id`/etc. in `SELECT|INSERT|UPDATE|DELETE|WHERE` — only test fixtures inserting into pre-rename baseline schema, which is correct).

### (c) Migration safety — final judgment

- `20260825000001` is defensively correct for both SQLite behaviors: trigger `keep_device_memory_limit` is dropped *before* any RENAME (section 1b), so the 3.51 "RENAME no longer rewrites trigger bodies" change cannot corrupt the schema; sqlx-bundled 3.50.x also works. New trigger recreated in section 5 with `thing_id`.
- Column renames cover 13 tables + `jobs.target_device_id`; `knowledge_relations` correctly excluded (no such column in baseline); `thing_templates.device_type` correctly DROP COLUMN (pre-existing `category` column + index) with index dropped first to satisfy DROP COLUMN constraints.
- tags rebuild: `UPDATE ... SET type='thing' WHERE type='device'` runs before rebuild (old CHECK still permits both values), full column list preserved in INSERT…SELECT, table name restored by RENAME so `tag_bindings.tag_id` FK reattaches by name; `pragma_foreign_key_check` = 0 asserted in `thing_rename_data_tests.rs`.
- Upgrade simulation is faithful: data test executes real `baseline.sql`, marks it applied with the correct embedded checksum, inserts samples across 8 rename surfaces (incl. `messages.device_type`, `agent_memories`, `agent_actions`), then runs real `run_migrations`.
- Ops path: backup via `VACUUM INTO` to `<db-dir>/backups/` (migrations.rs:160-186) only when migrations pending; post-migration FK enforcement aborts startup loudly; legacy-chain rejection pre-existing and unchanged. CHANGELOG documents auto-upgrade + backup. Self-consistent and sufficient for ops.

### (d) Test blind spots — incremental only (ledger items excluded)

- The `tag_bindings.target_type` seam (M1) has no test pinning either the tolerant or strict behavior — acceptable to defer to PR-2 where the data migration lands.
- No test asserts REST JSON key casing end-to-end for the renamed surfaces (`thingId`/`category`) — covered indirectly by handler tests that were updated; acceptable.

### (e) Reversibility/ops

`run_migrations` backup-then-migrate-then-FK-check chain is unchanged and self-consistent; CHANGELOG `[Unreleased]` documents the three BREAKING items plus MCP invariance and the three known follow-ups. Sufficient for an operator to execute the upgrade.

---

## Findings

### Minor

**M1 — tag_bindings target_type query asymmetry (PR-2 must fix code + data together)**
`crates/db/src/thing.rs:696` (`load_thing_tags_batch`) tolerates `target_type IN ('device','thing')`, but `thing.rs:1709, 1718, 1796, 1805` (find_devices `search_text` and `tag_name` filters) match only `target_type = 'device'`.
- Failure scenario: PR-2 (or any early-adopting client) writes a binding with `target_type='thing'`; batch tag loading sees it, but thing search by tag name / keyword silently misses it. No current behavioral error: every write path today (`demo.sql` seeds, free-form `BindTagRequest.target_type` from the unchanged frontend) still emits `'device'`.
- Action: ship-as-is for PR-1; in PR-2 land the planned data migration (`UPDATE tag_bindings SET target_type='thing' WHERE target_type='device'`, ledger Task-3 follow-up) **together with** flipping these 4 query sites.

**M2 — thing_template transitional `device_type` fields slipped their intended Task-6 handling**
`crates/db/src/thing_template.rs:93` (`TemplateQueryParams.device_type`) is accepted but silently ignored by SQL (only `category` filters, lines 701-702); `:126` (`CreateDeviceTemplateRequest.device_type: String`) is a **required** JSON field whose value is discarded on insert; `:147, :162, :197` similar.
- Failure scenario: an old API client filtering templates by `device_type` gets silently unfiltered results; new clients must send a dead mandatory `device_type` field when creating templates. Mitigation: the entire REST JSON surface is declared breaking and the old frontend is already broken by `deviceId→thingId`, so incremental harm is near zero.
- Action: ship-as-is; PR-2 removes or maps these fields (was ledger Task-4 deferred minor assigned to Task 6 — confirm it's on the PR-2 checklist).

**M3 — CHANGELOG omits two breaking items**
CHANGELOG lists `deviceId→thingId`/`deviceType→category` but not: (1) jobs REST JSON key `target_device_id`→`target_thing_id`; (2) `tags.type` CHECK no longer accepts `'device'` — old clients creating device-type tags now get a loud DB CHECK error.
- Failure scenario: API consumer relying on changelog alone misses two breaking changes. Loud failure for (2), silent key absence for (1).
- Action: ship-as-is (acceptable), or add one line each to the Unreleased section before merge if convenient.

**M4 — batch command request/response key asymmetry**
`crates/db/src/batch_command.rs:68` request still takes `device_ids` (JSON `device_ids`) while response items expose `thing_id` and `device_name`.
- Failure scenario: none behavioral; naming asymmetry only. `device_ids` is a request parameter, not a renamed DB column, so out of the rename mandate.
- Action: ship-as-is; PR-2 naming decision.

**M5 — dormant/legacy string keys retained (observations, no action needed)**
- `tenant/service.rs:14` `RESOURCE_TYPE_DEVICE = "device"` — internal quota resource key, no DB tie; renaming would be gratuitous.
- Seed `system.sql` now writes `"thing_group"` in plan features JSON; existing DBs keep `"device_group"` — grep confirms **zero code readers** of either key today, so dormant either way. When a reader appears (PR-2+), pick one key and migrate.
- Cosmetic deferred items confirmed cosmetic: perl-mangled comments (`thing/mod.rs`, `api/mod.rs:206` "things — 已迁移至 modules/device/handler/"), `access_control.rs` `EventType::Device(category)` binding names, duplicate `category, category` in two test INSERTs (SQLite tolerates; tests green), stale test name `delete_from_devices_works_after_migrations`.

---

## Deferred-minor triage (ledger)

| # | Ledger item | Verdict | Rationale |
|---|---|---|---|
| T1-a | schema tests don't assert 43 index renames | ship-as-is | Index DROP+CREATE with a wrong name/column fails loudly at migration apply time; no silent-wrong path. |
| T1-b | temp .db leak on assert failure | ship-as-is | Test hygiene only; $TMPDIR. |
| T2-a | tags sample lacks non-'device' control row | ship-as-is | UPDATE has a literal WHERE; schema CHECK + value assertions bound the mutation risk. |
| T2-b | 9 tables without data-preservation samples | ship-as-is | Mechanism proven on 8 representative tables covering FK, CHECK, trigger, paired renames. |
| T2-c | VACUUM INTO backup residue in temp_dir/backups | ship-as-is | Test artifact in tmp; production path backs up next to the DB file. |
| T3-a | demo.sql job-001 URL `/api/things/sync-status` nonexistent | ship-as-is | Demo seed only; cron 404 is benign; documented in CHANGELOG known follow-ups. |
| T3-b | tag_bindings.target_type data migration → PR-2 | ship-as-is | Documented; pair with M1 code sites in PR-2. |
| T3-c | permissions `'device:*'`, notification/event-security `'device'` strings → PR-2 | ship-as-is | Verified: only test fixtures (`role_handler_tests.rs:292`) and deliberate event-source semantics remain; no DB CHECK depends on them. |
| T4-a | JSON key authority note | ship-as-is | Informational; verified against d8c359a state directly. |
| T4-b | thing_template 5 `device_type` fields → Task 6 | **slipped** — see M2 | Still present at HEAD; ship-as-is under declared breaking, must be on PR-2 checklist. |
| T4-c | stale test name `delete_from_devices_works_after_migrations` | ship-as-is | Cosmetic. |
| T4-d | heartbeat.rs test JSON `thing_id` round-trip | ship-as-is | Harmless. |
| T4-e | `ensure_devices_table` column-set diff | ship-as-is | Verified at HEAD (thing.rs:3418): creates `things` with `category`; pre-existing minimal column set, edge-only bootstrap. |
| T6-a | duplicate `category, category` test SQL | ship-as-is | SQLite tolerates; tests green; cosmetic shape. |
| T6-b | access_control binding names | ship-as-is | Cosmetic. |
| T6-c | perl-mangled comments | ship-as-is | Cosmetic. |
| T6-d | report ripple-list incompleteness | ship-as-is | Process note; content verified correct. |
| T6-e | cron config key `thing_id` no data migration | ship-as-is | Explicitly documented as BREAKING in CHANGELOG. |
| T6-f | 3 Option-field MCP tests don't pin regression | ship-as-is | Serde alias pattern now covered by pool_adapter tests + MCP contract tests; attrs manually verified (`alarm_mcp.rs:50`, `device.rs:32,41,224`, `job.rs:33,60`). |
| T7-a | CHANGELOG `### Notes` non-standard section | ship-as-is | Cosmetic. |
| T7-b | 0-byte `data/tinyiothub.db.lock` residue | ship-as-is | Gitignored; harmless. |
| T7-c | libsqlite3-sys 3.51+ re-verify | ship-as-is | See residual risks. |

**None require fix-before-merge.**

---

## Residual risks for the human

1. **PR-2 coupling (highest attention):** the tag_bindings data migration must land in the same PR as the 4 strict query sites (M1), and the thing_template `device_type` fields (M2) must be removed/mapped. Recommend adding M1/M2/M3 explicitly to the PR-2 checklist — the ledger tracks the data migration but not the query-site flip or the template fields.
2. **SQLite 3.51+:** when libsqlite3-sys is upgraded past 3.50.x, re-run `cargo test -p db` (especially `thing_rename_*`). The migration is already defensively coded (trigger dropped before RENAME), so this is a verification gate, not an expected failure.
3. **Frontend PR-2 breaking list** should include beyond the declared two: `target_device_id→target_thing_id` (jobs), template `device_type` request fields (M2), `tags.type='device'` rejection (M3), batch `device_ids` asymmetry decision (M4).
4. **`device_group`/`thing_group` features key divergence** between seeded and existing DBs is dormant (no reader today) — resolve when a reader is introduced.
