-- AI 子系统梳理 Phase 2 Task 9：心跳历史迁移三件套（顺序=埋伏③）。
--
-- 背景：心跳 tick 历史以 agent_actions 行族存储——anchor 行
-- （action_type IN ('summary','error')，agent_id='__heartbeat__:<ws>'，
-- content JSON 含 {taskCount, result|error, trace}）+ 同 tick_id 的
-- proposal/auto_executed 行。统一 Run 内核后（T8 起 persist 支路直接写
-- agent_runs），历史 tick 必须回填才能在 run 读侧可见：trace 迁至
-- agent_runs.report，提案行改锚 run_id（C3：联结不断链）。
--
-- 顺序即正确性（埋伏③）：先合成 agent_runs 行，再改写提案 tick_id——
-- 反过来提案行的 EXISTS 锚检查找不到目标 run，改写全军覆没。
--
-- 幂等：Step 1 INSERT OR IGNORE（agent_runs.id 主键）；Step 2 前缀守卫
-- （tick_id NOT LIKE 'hbm\_%'）+ EXISTS 锚检查，重跑零副作用；Step 3
-- CREATE VIEW IF NOT EXISTS。
--
-- 范围说明：tick_id 为 NULL 的历史行（20260824000002 加列前/T22 锚行幂等
-- 之前的投影）无锚可合成 run_id，不在本迁移范围内，保持原样。
--
-- DDL-only 守卫：本迁移是 spec 裁定的一次性数据迁移
-- （docs/superpowers/specs/2026-10-12-ai-consolidation-design.md §4 N4），
-- 白名单见 scripts/guards/ddl-only.sh 的 SPEC_APPROVED 清单。
--
-- 逆迁移（回到迁移前形状；提案行 tick_id 无法还原，属单向数据迁移）：
--   DROP VIEW IF EXISTS heartbeat_tick_map;
--   DELETE FROM agent_runs WHERE id LIKE 'hbm\_%' ESCAPE '\';

-- ── Step 1（埋伏③ 先）：每个 anchor 行合成 agent_runs 行 ──
--   id          = 'hbm_' || tick_id（hbm_=heartbeat migration 合成前缀，
--                 与 T8 起 persist 支路写入的真实 run_id 不冲突）
--   trigger     = 'heartbeat:' || workspace_id（trigger_type 取其前缀
--                 'heartbeat'，与 trigger_type_of 约定一致）
--   outcome     = error→'failed'；summary→同 tick 存在提案/动作行则
--                 'acted'，否则 'no_action_needed'
--                 （词表见 crates/core/src/agent_runs.rs Outcome）
--   summary     = content.result（summary 行）/ content.error（error 行）
--   report      = RunReport 形状 JSON（trace 端点 live:false 分支按
--                 RunReport 反序列化）：trace 取自 anchor content，无
--                 trace 的历史行补空数组——"历史无 trace 不补"=不伪造
--                 过程记录；action_count=同 tick 提案+动作行数（T4
--                 count_actions_last_hour 依赖
--                 json_extract(report,'$.action_count')，insert_run
--                 落库同款补写先例）
--   created_at  沿用 anchor 行时间戳（时序/成本视图保持真实分布）
INSERT OR IGNORE INTO agent_runs
    (id, workspace_id, trigger_type, trigger_context, outcome, summary, report,
     verified, tool_calls, tokens, duration_ms, created_at)
SELECT
    'hbm_' || a.tick_id,
    a.workspace_id,
    'heartbeat',
    'heartbeat:' || a.workspace_id,
    CASE
        WHEN a.action_type = 'error' THEN 'failed'
        WHEN EXISTS (
            SELECT 1 FROM agent_actions s
            WHERE s.agent_id = a.agent_id AND s.tick_id = a.tick_id
              AND s.action_type IN ('proposal', 'auto_executed')
        ) THEN 'acted'
        ELSE 'no_action_needed'
    END,
    COALESCE(json_extract(a.content, '$.result'), json_extract(a.content, '$.error'), ''),
    json_object(
        'run_id',       'hbm_' || a.tick_id,
        'workspace_id', a.workspace_id,
        'trigger',      'heartbeat:' || a.workspace_id,
        'outcome',      CASE
            WHEN a.action_type = 'error' THEN 'failed'
            WHEN EXISTS (
                SELECT 1 FROM agent_actions s
                WHERE s.agent_id = a.agent_id AND s.tick_id = a.tick_id
                  AND s.action_type IN ('proposal', 'auto_executed')
            ) THEN 'acted'
            ELSE 'no_action_needed'
        END,
        'summary',      COALESCE(json_extract(a.content, '$.result'), json_extract(a.content, '$.error'), ''),
        'actions',      json('[]'),
        'verified',     json('false'),
        'duration_ms',  0,
        'tool_calls',   0,
        'tokens',       0,
        'trace',        json(COALESCE(json_extract(a.content, '$.trace'), '[]')),
        'action_count', (
            SELECT COUNT(*) FROM agent_actions s
            WHERE s.agent_id = a.agent_id AND s.tick_id = a.tick_id
              AND s.action_type IN ('proposal', 'auto_executed')
        )
    ),
    0, 0, 0, 0,
    a.created_at
FROM agent_actions a
WHERE a.agent_id LIKE '__heartbeat__:%'
  AND a.action_type IN ('summary', 'error')
  AND a.tick_id IS NOT NULL;

-- ── Step 2（埋伏③ 后）：提案/auto_executed 行改锚 run_id ──
-- EXISTS 锚检查：目标 run 行存在才改（无 anchor 的孤儿 tick 不动）；
-- 前缀守卫（NOT LIKE 'hbm\_%'）使重跑零副作用。anchor 行（summary/error）
-- 不在改写集内——它是 Step 3 视图的数据源。
UPDATE agent_actions
SET tick_id = 'hbm_' || tick_id
WHERE agent_id LIKE '__heartbeat__:%'
  AND tick_id IS NOT NULL
  AND tick_id NOT LIKE 'hbm\_%' ESCAPE '\'
  AND action_type IN ('proposal', 'auto_executed')
  AND EXISTS (
      SELECT 1 FROM agent_runs r
      WHERE r.id = 'hbm_' || agent_actions.tick_id
  );

-- ── Step 3：读侧兼容视图（过渡期 tick_id→run_id 映射）──
-- 数据源=anchor 行（其 tick_id 永不被 Step 2 改写），视图惰性求值；
-- 读侧切换（T10）完成后下版本 DROP。
CREATE VIEW IF NOT EXISTS heartbeat_tick_map AS
SELECT tick_id AS tick_id, 'hbm_' || tick_id AS run_id
FROM agent_actions
WHERE agent_id LIKE '__heartbeat__:%'
  AND action_type IN ('summary', 'error')
  AND tick_id IS NOT NULL;
