# Final Whole-Branch Review — thing-rename PR-2

- Branch: `refactor/thing-rename-pr2` (14 commits, cbfd1692..8c996915, stacked on PR-1 `refactor/thing-db-rename`)
- Reviewer: final broad pass (cross-cutting seams)
- Date: 2026-08-28

## Overall verdict: CLEAN TO MERGE

No Critical findings. No new defects introduced by the rename were found in any
cross-crate wire chain, the migration chain, the route tree, or the
frontend/backend contract. The two Important items are **pre-existing dead
paths** (present verbatim at base cbfd1692) that the rename carried through
unchanged — they are not regressions and per ledger ruling O1 ship as-is, but
they should be tracked as follow-ups with the new details below.

Gates were verified by task reports (workspace 1511/0, clippy, fmt, pnpm
type-check/build) and not re-run; all checks below are targeted reads/greps.

---

## Findings

### Important

**I-1. MQTT sub-thing discovery path is dead end-to-end (pre-existing, carried through rename; ledger O1 is only half the story)**
- `apps/cloud/src/shared/mqtt_client.rs:83` — cloud subscribes `tinyiothub/+/gateway/+/thing/discover`
- `apps/cloud/src/shared/mqtt_client.rs:200` — discover branch guard is `Some("thing") if parts.len() >= 7 && parts[5] == "discover"`
- `apps/cloud/src/domains/driver/gateway/service.rs:130` — PairingAck advertises `thing_discover = tinyiothub/{ws}/gateway/{gw}/thing/discover` (6 segments)
- `apps/edge/src/modules/gateway/service.rs:157-162` — edge `publish_discovery` publishes to `{prefix}/discovery` (5th segment `discovery`)

Failure scenario: three stacked mismatches, each independently fatal:
1. Edge publishes to `.../gateway/{gw}/discovery`, which cloud never subscribes (ledger O1).
2. Even a compliant edge publishing to the advertised/subscribed topic
   `.../gateway/{gw}/thing/discover` is dropped: that topic has **6** segments,
   but the router guard requires `parts.len() >= 7`, so it falls to `_ => None`.
   (The telemetry branch legitimately needs 7; the discover branch needs 6.)
3. Verified identical at base (`git show cbfd1692`): same guard, same advertised
   topic, same edge path — pre-existing, not a rename regression.

Ruling: ship-as-is per ledger O1, but the follow-up ticket must include the
`parts.len() >= 7` off-by-one — fixing only the topic strings will NOT make
discovery work.

**I-2. Gateway-level telemetry payload shape mismatch (pre-existing, not in ledger)**
- `apps/edge/src/modules/telemetry/service.rs:31-33` — edge publishes `serde_json::to_vec(&things)` (a raw JSON array) to `{prefix}/telemetry`
- `apps/cloud/src/shared/mqtt_client.rs:184-194` + `domains/driver/gateway/types.rs:91-98` — cloud parses `TelemetryMessage { type, data, timestamp }` (all required, `#[serde(rename = "type")]`)

Failure scenario: a JSON array can never deserialize into `TelemetryMessage`;
`serde_json::from_slice(...).ok()` yields `None` and every gateway telemetry
message is silently dropped. Verified byte-identical logic at base
(`git show cbfd1692:apps/edge/src/modules/telemetry/service.rs`) — pre-existing.
Ruling: ship-as-is (out of rename scope), track with I-1 as one
"gateway data uplink is dead" follow-up; add router-guard unit tests when fixed.

### Minor

**M-1. Agent tools catalog still advertises old MCP tool names (pre-existing since PR-1)**
- `crates/agent/src/tools/catalog.rs:163-171` — advertises `search_devices` / `get_device` / `create_device` / `delete_device`
- `apps/cloud/src/domains/mcp/tools/thing.rs:117,582,737,828` — actual registered names are `search_things` / `get_thing` / `create_thing` / `delete_thing`

Failure scenario: `GET /api/v1/tools/catalog` claims alignment with "the 16
MCP-registered handlers" (its own doc comment) but 4 of 7 thing-tool ids don't
match any registered tool; if `/tools/toggle` enforcement matches by MCP tool
name, disabling `delete_device` is a silent no-op against `delete_thing`.
Verified pre-existing at base (PR-1 renamed the handlers, catalog was already
stale). Frontend does not reference these names. Ruling: ship-as-is; rename the
4 ids (+ group id `device`) in the health/follow-up task — a PR whose goal is
"device→thing 收尾全量" is the natural owner.

**M-2. Historical heartbeat action rows lose thing id in UI (accepted degradation)**
- Writer (new): `crates/db/src/heartbeat.rs:367,391` writes `"thingId"` into `agent_actions.content`
- Reader: `apps/cloud/src/domains/agent/host/handler/workspace_heartbeat.rs:316,338,480,564` reads only `"thingId"` (no `deviceId` fallback)

Failure scenario: rows written before the upgrade contain `"deviceId"`; the
heartbeat history view shows blank thing id for them. New data is unaffected;
proposal execution is unaffected (pool_adapter keeps the 4-key fallback chain
`thing_id`→`thingId`→`deviceId`→`device_id` at
`domains/agent/host/pool_adapter.rs:87-98`, deliberately retained). Ruling:
ship-as-is (history display only; a data rewrite of JSON blobs is not worth it).

**M-3. Naming-debt residue bucket (all ship-as-is, health task)**
- `crates/core/src/types.rs:10` — `DeviceId` newtype (used by `mcp/tools/job.rs`; serde-transparent, no wire impact)
- `apps/cloud/src/domains/event/handler/overview.rs:40` — `top_things: Vec<DeviceEventCount>` element type not renamed (wire key is fine; field is a pre-existing always-empty placeholder, same at base)
- `crates/core/src/memory.rs:49` — `DeviceSnapshot`; test file names `device_dashboard_tests.rs` / `device_handler_tests.rs` / `device_profile_tests.rs`
- `crates/runtime/src/driver/drivers/modbus_driver.rs:150` — simulated driver accepts command name `"reset_device"` (user-defined thing-action names come from `thing_actions`; this stub list is pre-existing and untouched)
- `apps/cloud/src/domains/thing/tag/handler.rs:296-309` — `get_tag_stats` counts `tag_type == "device"` (dead branch, always 0; never emits a `thing` key). Frontend declares `byType` but no view renders it — invisible today.
- Alarm query param `device_ids` (`alarm/handler/mod.rs:103`, `alarm/dto.rs:246`; frontend `AlarmQueryParams.deviceIds`) — consistent both sides and explicitly documented as retained in CHANGELOG line 17.

---

## Area verdicts

### (a) Wire consistency (cross-crate) — PASS except pre-existing I-1/I-2
| Chain | Result |
|---|---|
| Pairing announce/ack | cloud `PairingAck.thing_id` (snake) ↔ edge reads `ack["thing_id"]` (`edge/.../pairing.rs:116`) ✓ |
| Pairing topics | advertised `GatewayTopics` (`thing_discover`/`thing_telemetry`) == cloud subscriptions == `subscribe_gateway()` (`mqtt_client.rs:336-349`) ✓ |
| Cloud→edge downlink | edge subscribes `/config/thing`,`/config`,`/command`,`/driver/install`; cloud only ever publishes `PairingAck` today (command downlink unimplemented — pre-existing, struct fields consistent) ✓ |
| MQTT discover/telemetry | FAIL — pre-existing dead paths, see I-1/I-2 |
| Heartbeat write/read | db `heartbeat.rs` writes `tool`/`toolName`/`thingId`/`deviceName` ↔ `workspace_heartbeat.rs` reads same keys ✓; pool_adapter 4-key fallback retained by design ✓ |
| LLM prompt vs parser | `loop_.rs:239-240` prompt and `report.rs:72,95` parser both `thing_id`-only (deliberate, no legacy fallback per plan ruling) ✓ |
| plugin-sdk FFI | `Thing`/`ThingCommand` snake_case fields; macros only use local var names; drivers/ are 3-line stubs (`driver_init` only) — no wire risk ✓ |
| MCP schema vs serde | input_schema keys `thingId`/`targetThingId` == serde `rename` + `alias = "deviceId"/"targetDeviceId"` (`mcp/tools/thing.rs:30,39`, `job.rs:33,60`); alias contract tests present (`phase2_tools_tests.rs:257-279`) ✓ |
| Sinks | postgres default column `thing_id`, influxdb tag `thing_id` (`storage/handlers/*.rs`) ✓ |
| Edge local REST | `/api/v1/things*` renamed; no in-repo consumer (outbound-only edge) ✓ |
| Thing event uplink | `thing/+/event/+` subscription ↔ `route_thing_event_message` parser ✓ |

### (b) Migration chain — PASS
- Full chain baseline→20260825000001→20260826000001→20260828000001 verified by reading; per-hop tests exist (`thing_rename_data_tests`, `thing_contract_data_tests`, `policy_action_rename_tests`) including FK check and prefix-similar value (`wipe_device_extra`) non-regression.
- Arithmetic verified: `substr(permission_id, 13)` strips `perm-device-` (12 chars); `substr(name, 8)` strips `device:` (7 chars). ✓
- Same-class scan for other persisted old-action-name/contract spots:
  - `workspaces.heartbeat_trust_config`, `workspace_autonomy_policy`, `policy_rules.target` — all covered by 20260828000001.
  - `agent_tools.tool_overrides` (JSON `{"enabled":[],"disabled":[]}` tool-name arrays) is the same class, but the feature has **no live readers** (only INSERT-default/DELETE and tests) — dead feature, no defect.
  - `agent_actions.content` history — M-2, accepted.
  - Trust classification (`crates/skills/src/trust.rs:29-39`) uses prefix/substring matching (`starts_with("wipe_")`, `contains("reboot")`) — robust to the rename itself. ✓
- Seeds flipped consistently (`seed/system.sql` perm-thing-*, `seed/demo.sql` target_type 'thing'); tag write boundary normalizes `'device'→'thing'` (`tag/handler.rs:321-326`, applied at both single and batch create). ✓

### (c) Frontend/backend contract — PASS (6 endpoints end-to-end)
`web/src/api/client.ts` auto-converts snake↔camel both directions, so key
equality at the snake level is what matters. Checked:
1. `GET /things/admin/overview` → `DashboardStats.total_things/online_things/monthly_growth.things` ↔ `dashboard.ts:132-146` + `types/index.ts:389-400` ✓ (ledger's `monthlyGrowth.devices` drift was fixed in 8d8250c3)
2. `GET /things` → `{items,total,limit,offset,unassigned_resource_count}` ↔ `ThingListResponse` ✓; `ThingResponse.category` ↔ `Thing.category` ✓
3. `GET /things/admin/distribution` + `/quick` ↔ `dashboard.ts:37-39` ✓
4. `GET /things/admin/{id}/metrics` ↔ `monitoring.ts:18` ✓
5. Workspace `thing_count` — backend renamed; no frontend consumer (workspace view is chat-based) ✓
6. `SystemFeatures.enable_thing_management`/`max_things` — renamed; no frontend consumer ✓
Extra: `SystemTraceOverview.active_things` has no web consumer; a2ui catalog registers `ThingCard/ThingTable` + transitional `DeviceCard/DeviceTable` aliases matching the canvas tool prompt (`canvas.rs:35`); `AlarmCard(...,deviceName,...)` prompt ↔ frontend `alarm-card.ts` (still `deviceName`) consistent.

### (d) Route tree final state — PASS
- `.nest("/things", thing)` + `.nest("/things/admin", admin)` coexist; matchit static-priority rules put `/things/admin/**` ahead of the `/things/{*rest}` wildcard, and inside the thing router static segments (`templates`, `resources`, `import`) beat `/{id}`. Verified live by passing tests hitting `/api/v1/things/admin/distribution|quick|{id}/...` (`device_dashboard_tests.rs:33,54`, `device_handler_tests.rs:377-521`).
- Tombstone: `management.rs` catch-all `any()` on `/` + `/{*rest}` under `/devices`, plus explicit bare-`/devices/` route in `api/mod.rs:42-45` (nest does not forward the trailing-slash path) — every method and subpath 410s; automated test `test_removed_device_endpoints_return_410` (`thing_handler_tests.rs:293-311`).
- No shadow found: admin router has no route colliding with thing main router; `/{thing_id}` vs `/overview`/`/performance/*` statics inside admin monitoring resolve static-first.

### (e) Ledger deferred-minor triage
**Resolved in-branch (verify-only, all confirmed):** 410 tombstone tests (T4→T7); tombstone catch-all 404/405 gap (T7); `monthlyGrowth.devices` drift (T6→8d8250c3); `things.ts` noise expression + marketplace duplicate rows (T6→8d8250c3); M2 edge-credential wording — CHANGELOG line 11 now explicitly says old credential files (`device_id` key) are invalidated and edge must re-pair; `enable_thing_management` flip (T4 ruling→8d8250c3, no frontend consumer).

**Ship-as-is (deferred to health task / documented):**
- `get_tag_stats` dead `device` branch — invisible (no UI renders `byType`); health task.
- permission.rs dead-column query functions — if ever called they fail loudly; tests green ⇒ dead; health task.
- `ThingEventType::DeviceAlarm` mixed naming — deliberate wire-compat ruling (serde values `device_alarm` etc. kept); record in naming-conventions doc.
- Error-message/UI display strings ("设备" wording etc.) — non-contract, disclosed.
- Report count/line-number drift (T3/T4) — documentation only.
- Comment/test-name residue (`thing_property.rs` comments were partially cleaned in 8d8250c3; test file names remain) — naming hygiene, health task.
- O1 edge discovery dead path — ship, but escalate with I-1's new guard detail.
- O2 examples/* not in workspace members — repo-level, unchanged.
- `thing-cache.updateProperty` name/id gap — verified zero callers (dead code); health task delete or fix.
- `/workspaces/{id}/knowledge/*` frontend API without backend route — pre-existing, unrelated to rename.
- Tombstone message text `/api/devices` (vs `/api/v1/devices`) — pre-existing wording; plan mandated 文案不变.
- apps/core-side `Device*` type residue (DeviceId/DeviceSnapshot/DeviceEventCount/DeviceFilterRequest etc.) — health task batch (M-3).

### (f) Incremental test blind spots (worth adding, none merge-blocking)
1. Edge pairing ack parse (`edge/.../pairing.rs:116`) has no unit test; tolerant `unwrap_or_default()` means a missing `thing_id` silently yields empty credentials. A 10-line JSON parse test pins the co-upgrade contract.
2. No test pins the MQTT `route_data_message` segment guards — exactly why I-1's off-by-one survives. Add when the discovery path is repaired.
3. No read-back test for pre-upgrade `agent_actions` content (`deviceId`) — accepted degradation (M-2); add only if a compat fallback is ever introduced.

---

## Residual risks for the human
1. **Gateway data uplink is dead in production today** (discovery I-1 + gateway telemetry I-2), and was before this PR. If anyone believes sub-device discovery/telemetry works, it does not — prioritize the follow-up; the fix is small (edge topic, router guard, payload wrap) but spans edge+cloud and needs the co-upgrade story this PR just established.
2. **edge must co-upgrade with cloud** (wire keys, topics, credential file, edge REST path) — CHANGELOG documents it; make sure deployment runbooks enforce it, since failure modes are silent (MQTT messages just stop parsing).
3. **Tools catalog vs MCP names** (M-1) — if the tool-toggle UI is ever wired to enforcement, the 4 stale ids become a real authorization-gap bug rather than cosmetic debt.
4. Old heartbeat history shows blank thing column (M-2) — set support expectations; no action needed.
