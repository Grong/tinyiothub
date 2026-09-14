-- 20260914000001: judgments 硬化（CEO+eng 评审 2026-09-14 裁定，PR #96 追加）
-- 1) status CHECK 加第 9 态 'dispatch_suppressed'：dispatch 被 O11 dedup/队列
--    拦下的终态标记（不开票、不计入日预算——零 LLM 调用不耗额度）。
-- 2) 新增 triage_mode 快照列：judgment 创建时按 workspace 配置快照
--    （'annotate'|'suppress'），verdict 路由按快照而非到达时配置（S4/T-20：
--    中途切模式不影响在途判断，审计链自洽）。
-- 3) 新索引 (status, judged_at)：三态 SLA 清扫器的全表过滤查询用。
-- SQLite 不能改 CHECK/加带默认的非空列约束语义——重建表（judgments 为 P0
-- 新表，重建零存量风险；migration runner 全程 FK OFF）。

CREATE TABLE judgments_new (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    alarm_id TEXT REFERENCES thing_alarms(id),
    -- 无 FK：RunRecorded 广播到多个订阅者，judgment 订阅者回填 run_id 时
    -- agent_runs 行可能尚未由 persist 订阅者落库（并发序不保证）。
    run_id TEXT,
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
         'resolved','escalated','investigation_failed','budget_skipped',
         'dispatch_suppressed')),
    -- 创建时从 workspace heartbeat_config.triage_mode 快照（T-20/S4）；
    -- 'annotate'=只记录判断不动报警（影子期默认）；'suppress'=noise 判可抑制
    -- Warning/Info 报警。NULL 视为 'annotate'（保守默认）。
    triage_mode TEXT NOT NULL DEFAULT 'annotate'
        CHECK (triage_mode IN ('annotate','suppress')),
    -- 状态进入时刻（SLA 清扫的起算点）：每次状态翻转更新。与 judged_at
    -- 分开——judged_at 服务延迟指标（created→judged），不可被执行态覆盖。
    state_entered_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    judged_at TEXT,
    resolved_at TEXT
);

INSERT INTO judgments_new SELECT
    id, workspace_id, alarm_id, run_id, ticket_id, proposal_id, thing_id,
    verdict, reason, evidence_json, suggested_action, action_category, status,
    'annotate', COALESCE(judged_at, created_at), created_at, judged_at, resolved_at
FROM judgments;
DROP TABLE judgments;
ALTER TABLE judgments_new RENAME TO judgments;

-- feed 页主查询：workspace + status 筛选 + 时间倒序
CREATE INDEX judgments_ws_status_created ON judgments(workspace_id, status, created_at);
CREATE INDEX judgments_alarm ON judgments(alarm_id);
-- 同一报警同时最多一个未终态判断（去重窗口之外的保险）
CREATE UNIQUE INDEX judgments_active_alarm ON judgments(alarm_id)
    WHERE status IN ('investigating','awaiting_approval','executing');
-- 三态 SLA 清扫器：status + judged_at 全表过滤（E1/4A）
CREATE INDEX judgments_open_sla ON judgments(status, judged_at);
