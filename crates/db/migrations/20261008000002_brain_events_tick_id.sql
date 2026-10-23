-- AI 运行透明化 Task 2：brain_events 视图重建——各分支补 tick_id 列。
-- patrol 提案行投影 agent_actions.tick_id（Task 9 起写入方填；历史提案行 NULL）；
-- alarm / directive / tick 聚合行无 tick 来源，恒 NULL。
-- 证据连接键保持 created_at 不变（历史提案行 tick_id 为 NULL，按 tick_id 连接
-- 永不命中——2026-09-19 实测；tick_id 投影只为透明化展示，不改证据语义）。
--
-- 逆迁移（去掉 tick_id 列，回到 20260919000001 形状）：
--   DROP VIEW IF EXISTS brain_events;
--   CREATE VIEW brain_events AS <20260919000001_brain_events_view_fix.sql 的
--   SELECT 全文，即本文件去掉所有 tick_id 投影行后的版本>;
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
  NULL AS tick_id,
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
  a.tick_id AS tick_id,       -- Task 2 新增投影；Task 9 起写入方填，历史行 NULL
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
  -- 证据契约 v2 的 {"summary": ...} 形状。连接键保持 created_at（历史提案行
  -- tick_id 为 NULL，按 tick_id 连接永不命中——2026-09-19 实测）。
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
  NULL AS tick_id,
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
-- 列序（title 之后）：verdict, reason, suggested_action, action_category,
-- action_params, risk, run_id, tick_id, ticket_id——tick_id 恒 NULL。
SELECT
  'tick:' || a.workspace_id || ':' || strftime('%Y-%m-%dT%H', a.created_at) AS id,
  a.workspace_id,
  'patrol_tick' AS source,
  NULL, NULL,
  '巡检正常 · 本小时 ' || COUNT(DISTINCT a.tick_id) || ' 次巡检无发现' AS title,
  NULL, '', NULL, NULL, NULL, NULL, NULL, NULL, NULL,
  'patrol_ok' AS status,
  'annotate',
  strftime('%Y-%m-%dT%H:00:00', a.created_at) AS created_at,
  NULL, strftime('%Y-%m-%dT%H:00:00', a.created_at), NULL,
  '{}' AS evidence_json
FROM agent_actions a
WHERE a.action_type = 'summary'
GROUP BY a.workspace_id, strftime('%Y-%m-%dT%H', a.created_at);
