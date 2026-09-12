-- T2: AI 处置判断（judgments）——大脑主干化 P0 的核心数据模型
-- 设计：~/.gstack/projects/Grong-tinyiothub/chenguorong-...-design-20260911-161121.md
--
-- 状态机（Design-Review 修订版）：
--   investigating（调查 run 进行中/排队）
--     ├─ verdict=noise          → noise_archived（报警 suppress，静默归档）
--     ├─ verdict=self_healable  → awaiting_approval ─批准→ executing ─成功→ resolved
--     │                             │                      └失败→ escalated
--     │                             ├─拒绝（必填原因）→ escalated
--     │                             └─24h 超时（cron）→ escalated
--     ├─ verdict=needs_human    → escalated（转工单）
--     ├─ 调查失败/解析失败        → investigation_failed（转工单）
--     └─ 超日预算                → budget_skipped（报警保持 Active 走人工路径）
-- 所有状态翻转用条件更新（UPDATE ... WHERE status='预期值'）防并发互撞。

CREATE TABLE judgments (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    alarm_id TEXT REFERENCES thing_alarms(id),
    run_id TEXT REFERENCES agent_runs(id),      -- 调查 run，run 完成前可 NULL
    ticket_id INTEGER REFERENCES tickets(id),   -- escalated 时的工单
    proposal_id TEXT,                           -- awaiting_approval 时的审批提案
    thing_id TEXT,
    verdict TEXT CHECK (verdict IN ('noise','self_healable','needs_human')),
    reason TEXT NOT NULL DEFAULT '',            -- 人话理由
    evidence_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(evidence_json)),
    suggested_action TEXT,
    action_category TEXT CHECK (action_category IN
        ('device_reboot','connection_recovery','property_adjust','threshold_tuning','other')),
    status TEXT NOT NULL DEFAULT 'investigating' CHECK (status IN
        ('investigating','noise_archived','awaiting_approval','executing',
         'resolved','escalated','investigation_failed','budget_skipped')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    judged_at TEXT,
    resolved_at TEXT
);

-- feed 页主查询：workspace + status 筛选 + 时间倒序
CREATE INDEX judgments_ws_status_created ON judgments(workspace_id, status, created_at);
CREATE INDEX judgments_alarm ON judgments(alarm_id);
-- 同一报警同时最多一个未终态判断（去重窗口之外的保险）
CREATE UNIQUE INDEX judgments_active_alarm ON judgments(alarm_id)
    WHERE status IN ('investigating','awaiting_approval','executing');

-- 反馈独立表（D11 裁决：P0 就建，全历史；「改判取最新」由查询层取每判断最新行）
CREATE TABLE judgment_feedback (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    judgment_id TEXT NOT NULL REFERENCES judgments(id),
    workspace_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    verdict TEXT NOT NULL CHECK (verdict IN ('right','wrong')),
    reason TEXT,                                -- verdict='wrong' 时 service 层强制非空（≥4 字符）
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX judgment_feedback_judgment ON judgment_feedback(judgment_id, created_at);
CREATE INDEX judgment_feedback_ws ON judgment_feedback(workspace_id, created_at);
