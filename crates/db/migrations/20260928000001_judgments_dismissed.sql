-- 20260928000001: judgments status CHECK 加第 10 态 'dismissed'（X5，CEO 评审
-- D4 裁定）：两路径拒绝落 dismissed 不开票（v3「用户已定」语义）；关联报警
-- 3 天仍 Active 由审批超时清扫器重浮为 investigating。
-- SQLite 不能改 CHECK——重建表（20260914000001 同款先例；runner 全程 FK OFF）。

DROP VIEW IF EXISTS brain_events;

CREATE TABLE judgments_new (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    alarm_id TEXT REFERENCES thing_alarms(id),
    run_id TEXT,
    ticket_id INTEGER REFERENCES tickets(id),
    proposal_id TEXT,
    thing_id TEXT,
    verdict TEXT CHECK (verdict IN ('noise','self_healable','needs_human')),
    reason TEXT NOT NULL DEFAULT '',
    evidence_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(evidence_json)),
    suggested_action TEXT,
    action_category TEXT CHECK (action_category IN
        ('device_reboot','connection_recovery','property_adjust','threshold_tuning','other')),
    status TEXT NOT NULL DEFAULT 'investigating' CHECK (status IN
        ('investigating','noise_archived','awaiting_approval','executing',
         'resolved','escalated','investigation_failed','budget_skipped',
         'dispatch_suppressed','dismissed')),
    triage_mode TEXT NOT NULL DEFAULT 'annotate'
        CHECK (triage_mode IN ('annotate','suppress')),
    state_entered_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    judged_at TEXT,
    resolved_at TEXT
);

INSERT INTO judgments_new SELECT
    id, workspace_id, alarm_id, run_id, ticket_id, proposal_id, thing_id,
    verdict, reason, evidence_json, suggested_action, action_category, status,
    triage_mode, state_entered_at, created_at, judged_at, resolved_at
FROM judgments;
DROP TABLE judgments;
ALTER TABLE judgments_new RENAME TO judgments;

CREATE INDEX judgments_ws_status_created ON judgments(workspace_id, status, created_at);
CREATE INDEX judgments_alarm ON judgments(alarm_id);
CREATE UNIQUE INDEX judgments_active_alarm ON judgments(alarm_id)
    WHERE status IN ('investigating','awaiting_approval','executing');
CREATE INDEX judgments_open_sla ON judgments(status, state_entered_at);

-- brain_events 视图引用 judgments——重建表会先失效视图；按
-- 20260919000001 的定义原样重建（迁移快照，复制是惯例）。
DROP VIEW IF EXISTS brain_events;
CREATE VIEW brain_events AS
SELECT
  'alarm:' || j.id AS id,
  j.workspace_id,
  'alarm' AS source,
  j.alarm_id,
  j.thing_id,
  COALESCE((SELECT a.alarm_message FROM thing_alarms a WHERE a.id = j.alarm_id), '') AS title,
  j.verdict,
  j.reason,
  j.suggested_action,
  j.action_category,
  NULL AS action_params,
  (SELECT CASE a.alarm_level WHEN 'critical' THEN 'high' WHEN 'error' THEN 'high'
          WHEN 'warning' THEN 'medium' ELSE 'low' END
   FROM thing_alarms a WHERE a.id = j.alarm_id) AS risk,
  j.run_id,
  j.ticket_id,
  CASE j.status WHEN 'noise_archived' THEN 'self_closed' ELSE j.status END AS status,
  j.triage_mode,
  j.created_at,
  j.judged_at,
  j.state_entered_at,
  j.resolved_at,
  j.evidence_json
FROM judgments j

UNION ALL

-- patrol 提案（agent_actions JSON 行；每提案一行，不是一个 tick 一行）
SELECT
  'patrol:' || json_extract(a.content, '$.proposalId') AS id,
  a.workspace_id,
  'patrol' AS source,
  NULL AS alarm_id,
  json_extract(a.content, '$.thingId') AS thing_id,
  COALESCE(json_extract(a.content, '$.summary'), '') AS title,
  NULL AS verdict,
  COALESCE(json_extract(a.content, '$.reason'), '') AS reason,
  NULL AS suggested_action,   -- title（提案 summary）即建议；toolName 是机器名不给人看
  NULL AS action_category,
  json_extract(a.content, '$.parameters') AS action_params,
  CASE lower(COALESCE(json_extract(a.content, '$.risk'), ''))
       WHEN 'low' THEN 'low' WHEN 'medium' THEN 'medium' WHEN 'high' THEN 'high'
       ELSE NULL END AS risk,
  NULL AS run_id,
  NULL AS ticket_id,
  CASE json_extract(a.content, '$.status')
       WHEN 'pending' THEN 'awaiting_approval'
       WHEN 'approved' THEN 'resolved'
       WHEN 'rejected' THEN 'dismissed'
       ELSE 'awaiting_approval' END AS status,
  'annotate' AS triage_mode,
  a.created_at,
  NULL AS judged_at,          -- patrol 无 24h SLA（judged_at=NULL 不被清扫命中）
  a.created_at AS state_entered_at,
  NULL AS resolved_at,
  -- 证据：同 tick 锚行（action_type='summary'）的 result 文本，归一化到
  -- 证据契约 v2 的 {"summary": ...} 形状。连接键 = created_at（提案行
  -- tick_id 恒 NULL，按 tick_id 连接永不命中——2026-09-19 实测）。
  COALESCE((SELECT json_object('summary', json_extract(s.content, '$.result'))
            FROM agent_actions s
            WHERE s.workspace_id = a.workspace_id AND s.created_at = a.created_at
              AND s.action_type = 'summary' LIMIT 1), '{}') AS evidence_json
FROM agent_actions a
WHERE a.action_type = 'proposal'

UNION ALL

-- directive：chat 用户指令 run（problem_key IS NULL 且 trigger_type='user'）
SELECT
  'directive:' || r.id AS id,
  r.workspace_id,
  'directive' AS source,
  NULL AS alarm_id,
  json_extract(r.report, '$.thing_id') AS thing_id,
  COALESCE(NULLIF(json_extract(r.report, '$.trigger'), ''), substr(r.summary, 1, 80)) AS title,
  NULL AS verdict,
  COALESCE(substr(r.summary, 1, 200), '') AS reason,
  NULL AS suggested_action,
  NULL AS action_category,
  NULL AS action_params,
  NULL AS risk,
  r.id AS run_id,
  NULL AS ticket_id,
  CASE r.outcome WHEN 'failed' THEN 'investigation_failed' ELSE 'resolved' END AS status,
  'annotate' AS triage_mode,
  r.created_at,
  r.created_at AS judged_at,
  r.created_at AS state_entered_at,
  r.created_at AS resolved_at,
  r.report AS evidence_json      -- RunReport 全文即完整审计
FROM agent_runs r
WHERE r.problem_key IS NULL AND r.trigger_type = 'user'

UNION ALL

-- tick 聚合行（每小时一条，「AI 活着且查了没事」的负证据）
SELECT
  'tick:' || a.workspace_id || ':' || strftime('%Y-%m-%dT%H', a.created_at) AS id,
  a.workspace_id,
  'patrol_tick' AS source,
  NULL, NULL,
  '巡检正常 · 本小时 ' || COUNT(DISTINCT a.tick_id) || ' 次巡检无发现' AS title,
  NULL, '', NULL, NULL, NULL, NULL, NULL, NULL,
  'patrol_ok' AS status,
  'annotate',
  strftime('%Y-%m-%dT%H:00:00', a.created_at) AS created_at,
  NULL, strftime('%Y-%m-%dT%H:00:00', a.created_at), NULL,
  '{}' AS evidence_json
FROM agent_actions a
WHERE a.action_type = 'summary'
GROUP BY a.workspace_id, strftime('%Y-%m-%dT%H', a.created_at);
