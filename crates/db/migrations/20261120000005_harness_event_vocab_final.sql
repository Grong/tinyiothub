UPDATE session_events SET type = 'user/message'
WHERE type = 'message.created' AND json_extract(payload_json, '$.role') = 'user';
UPDATE session_events SET type = 'assistant/message'
WHERE type = 'message.created' AND json_extract(payload_json, '$.role') = 'assistant';

UPDATE session_events SET type = 'todo/write' WHERE type = 'todo.updated';
UPDATE session_events SET type = 'question/asked' WHERE type = 'question.asked';
UPDATE session_events SET type = 'question/answered' WHERE type = 'question.answered';
UPDATE session_events SET type = 'approval/requested' WHERE type = 'approval.requested';
UPDATE session_events SET type = 'approval/resolved' WHERE type = 'approval.resolved';
UPDATE session_events SET type = 'session/start' WHERE type = 'session.started';
UPDATE session_events SET type = 'session/end' WHERE type = 'session.ended';
UPDATE session_events SET type = 'trigger/received' WHERE type = 'trigger.received';
UPDATE session_events SET type = 'wake/dispatched' WHERE type = 'wake.dispatched';
UPDATE session_events SET type = 'skill/loaded' WHERE type = 'skill.loaded';
