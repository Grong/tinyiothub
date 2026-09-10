-- 工单模块 M1（设计：~/.gstack/projects/Grong-tinyiothub/chenguorong-feat-ticket-escalation-design-20260909-113824.md）
-- Agent→人 升级原语：自治 run 失败（outcome∈{failed,budget_exceeded,rejected}）由订阅者开票。
--
-- 状态机（M1，reopen 延至 M2）：
--   open ──claim──> claimed ──start──> in_progress ──resolve──> resolved ──close──> closed
--     │                │                                                     ▲
--     │                └──────abandon──────> open（清 assignee/claimed_at）     │
--     └──────────────────close（无效工单）─────────────────────────────────────┘
-- 所有迁移用条件更新（UPDATE ... WHERE state='预期值'）防并发互撞。
--
-- 去重：同一故障的【活跃】工单唯一（open/claimed/in_progress；resolved 后复发
-- 开新票 = "上次修复未生效"的信号，是特性）。failure_hash 由订阅者按触发源
-- 计算（UserDirective→problem_key / ThingEvent→dedup_key / 其余→
-- hash(end_reason||normalize(last_error))）。COALESCE 解决 SQLite NULL
-- 互不相等导致 thing_id=NULL（workspace 级失败）行不去重的问题。

CREATE TABLE tickets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    thing_id TEXT,                    -- workspace 级 Agent 失败时为 NULL
    agent_run_id TEXT NOT NULL,       -- 首个触发 run；复发 run 记 ticket_events
    session_key TEXT UNIQUE REFERENCES chat_sessions(session_key),  -- M2 对话用，M1 恒 NULL
    title TEXT NOT NULL,
    briefing TEXT NOT NULL CHECK (json_valid(briefing)),
    failure_hash TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'open'
        CHECK (state IN ('open','claimed','in_progress','resolved','closed')),
    assignee_id TEXT,
    resolution_text TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    claimed_at TEXT,
    resolved_at TEXT,
    closed_at TEXT,
    reopened_at TEXT                  -- M2 预留
);

-- 活跃工单去重（见顶部注释）
CREATE UNIQUE INDEX tickets_active_dedup
    ON tickets(COALESCE(thing_id, ''), failure_hash)
    WHERE state IN ('open', 'claimed', 'in_progress');

-- 列表页 + 未认领计数 badge
CREATE INDEX tickets_ws_state_created ON tickets(workspace_id, state, created_at);

CREATE TABLE ticket_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ticket_id INTEGER NOT NULL REFERENCES tickets(id),
    kind TEXT NOT NULL CHECK (kind IN ('state_change','system')),  -- M2 如需 comment 再扩
    actor_type TEXT NOT NULL CHECK (actor_type IN ('user','agent','system')),
    actor_id TEXT,                    -- user_id 或 agent run_id
    payload TEXT CHECK (payload IS NULL OR json_valid(payload)),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX ticket_events_ticket ON ticket_events(ticket_id, created_at);
