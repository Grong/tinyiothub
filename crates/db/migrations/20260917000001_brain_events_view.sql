-- brain_events 只读投影（P0 不建物理表不双写）：
-- UNION judgments ∪ agent_actions(提案) ∪ agent_runs(指令) ∪ agent_actions(tick 聚合)。
-- 状态改名只在本层：noise_archived→self_closed；patrol rejected→dismissed。
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
  json_extract(a.content, '$.toolName') AS suggested_action,
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
  -- 证据：同 tick 锚行（action_type='summary'）内容 + 本提案 JSON
  COALESCE((SELECT s.content FROM agent_actions s
            WHERE s.workspace_id = a.workspace_id AND s.tick_id = a.tick_id
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
