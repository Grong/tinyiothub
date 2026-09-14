#!/usr/bin/env python3
"""演示种子：一键重现「10 条报警进来」的产品故事（E3，CEO 评审 2026-09-14）。

用法：
    python3 scripts/demo_seed_dispositions.py [--db tinyiothub.db] [--workspace WS]

效果（处置中心 feed 立即可见）：
    4 条噪声已归档（annotate 模式，只标注不抑制）
    3 条可自愈待审批（含 24h 倒计时）
    3 条需人工已转工单（带调查结论简报）
演示动线：feed 头部计数 → 批准一条看执行流转 → ✕ 点错一条噪声看报警恢复+重查。

注意：这是确定性种子（绕过真实 LLM 调查），用于演示 feed 与工单形态；
真实全链路演示用真实设备报警触发 enter_disposition。
"""
import argparse
import sqlite3
import sys
import uuid
from datetime import datetime, timedelta, timezone

NOISE = [
    ("温度短暂越限后自行回落", "温度传感器 31.2°C（阈值 30°C），40 秒后回落"),
    ("湿度单点抖动", "湿度 61% 单次采样越限，下一采样 58%"),
    ("网关闪断 8 秒自恢复", "设备离线 8 秒后自动重连，历史每周 2-3 次"),
    ("备份窗口 CPU 峰值", "CPU 82% 持续 20 秒，每日备份窗口常态"),
]
APPROVAL = [
    ("网关离线 4 分钟，可重连恢复", "历史同类 12 次中 10 次重连恢复", "connection_recovery", "恢复设备连接"),
    ("MQTT 订阅掉线", "broker 正常，last-will 已触发，重订阅可恢复", "connection_recovery", "恢复设备连接"),
    ("磁盘 89% 超阈值", "日志轮转配置存在但未执行", "other", "触发日志轮转"),
]
HUMAN = [
    ("模具温度持续上升且冷却水流量下降", "冷却系统疑似故障，需现场检查"),
    ("设备离线 2 小时重连 5 次失败", "持续离线，需现场查看电源"),
    ("压力容器超安全上限", "9.2bar（上限 8.5bar），泄压阀状态未知"),
]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--db", default="tinyiothub.db")
    ap.add_argument("--workspace", default="workspace-default-001")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    con.execute("PRAGMA foreign_keys = ON")
    ws = args.workspace
    now = datetime.now(timezone.utc)

    ws_row = con.execute("SELECT id FROM workspaces WHERE id = ?", (ws,)).fetchone()
    if not ws_row:
        print(f"workspace {ws} 不存在——先启动一次服务完成初始化", file=sys.stderr)
        return 1

    thing_id = f"demo-thing-{uuid.uuid4().hex[:8]}"
    rule_id = f"demo-rule-{uuid.uuid4().hex[:8]}"
    con.execute(
        "INSERT INTO things (id, workspace_id, name, thing_type, state, created_at, updated_at)"
        " VALUES (?, ?, '演示设备', 'sensor', 1, ?, ?)",
        (thing_id, ws, now.isoformat(), now.isoformat()),
    )
    con.execute(
        "INSERT INTO thing_alarm_rules (id, thing_id, rule_name, rule_type, condition_config, alarm_level, workspace_id)"
        " VALUES (?, ?, '演示温度阈值', 'threshold', '{}', 'warning', ?)",
        (rule_id, thing_id, ws),
    )

    def mk_alarm(i: int, level: str, msg: str, minutes_ago: int) -> str:
        aid = f"demo-alarm-{uuid.uuid4().hex[:8]}"
        t = (now - timedelta(minutes=minutes_ago)).isoformat()
        con.execute(
            "INSERT INTO thing_alarms (id, thing_id, rule_id, alarm_level, alarm_message, alarm_time, workspace_id)"
            " VALUES (?, ?, ?, ?, ?, ?, ?)",
            (aid, thing_id, rule_id, level, msg, t, ws),
        )
        return aid

    def mk_judgment(aid: str, status: str, verdict: str | None, reason: str, minutes_ago: int,
                    category: str | None = None, action: str | None = None, ticket_id=None) -> str:
        jid = str(uuid.uuid4())
        created = (now - timedelta(minutes=minutes_ago)).isoformat()
        judged = (now - timedelta(minutes=max(minutes_ago - 2, 0))).isoformat()
        con.execute(
            "INSERT INTO judgments (id, workspace_id, alarm_id, thing_id, verdict, reason,"
            " evidence_json, suggested_action, action_category, status, triage_mode,"
            " state_entered_at, created_at, judged_at, ticket_id)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'annotate', ?, ?, ?, ?)",
            (jid, ws, aid, thing_id, verdict, reason,
             '{"source":"demo","excerpt":"演示证据摘录"}', action, category,
             status, judged, created, judged, ticket_id),
        )
        return jid

    def mk_ticket(title: str, problem: str) -> int:
        cur = con.execute(
            "INSERT INTO tickets (workspace_id, thing_id, agent_run_id, title, briefing, failure_hash)"
            " VALUES (?, ?, ?, ?, ?, ?)",
            (ws, thing_id, f"demo-{uuid.uuid4().hex[:8]}", title,
             f'{{"problem": "{problem}", "source": "demo_seed"}}', f"demo-{uuid.uuid4().hex[:8]}"),
        )
        return cur.lastrowid  # type: ignore[return-value]

    for i, (title, detail) in enumerate(NOISE):
        aid = mk_alarm(i, "info", title, 50 - i * 5)
        con.execute("UPDATE thing_alarms SET is_suppressed = 0 WHERE id = ?", (aid,))  # annotate：不抑制
        mk_judgment(aid, "noise_archived", "noise", f"{detail}——正常波动，已静默归档", 50 - i * 5)

    for i, (title, detail, cat, action) in enumerate(APPROVAL):
        aid = mk_alarm(10 + i, "warning", title, 30 - i * 5)
        mk_judgment(aid, "awaiting_approval", "self_healable",
                    f"{detail}。建议：{action}（待审批，24h 未响应自动转工单）",
                    30 - i * 5, category=cat, action=action)

    for i, (title, detail) in enumerate(HUMAN):
        aid = mk_alarm(20 + i, "critical", title, 20 - i * 5)
        tid = mk_ticket(f"需人工：{title}", detail)
        mk_judgment(aid, "escalated", "needs_human", f"{detail}。已转工单。", 20 - i * 5, ticket_id=tid)

    con.commit()
    print(f"✅ 演示数据已注入 workspace={ws} thing={thing_id}")
    print("   处置中心（/dispositions）应见：3 条待审批 + 3 条需人工；「噪声」tab 4 条已归档")
    return 0


if __name__ == "__main__":
    sys.exit(main())
