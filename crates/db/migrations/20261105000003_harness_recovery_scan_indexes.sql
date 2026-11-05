-- Recovery matrix guard is on the dispatch/consume hot path. It only needs
-- actionable rows: non-terminal effects and non-terminal commands. Terminal
-- rows cannot be dispatched or consumed again, so they are excluded from the
-- scan entirely.
-- The `NOT IN` form also keeps corrupt/unknown persisted states visible to the
-- fail-closed guard while excluding known terminal rows.
CREATE INDEX IF NOT EXISTS idx_effect_intents_recovery_scan
    ON effect_intents (created_at, effect_id)
    WHERE state NOT IN ('completed', 'failed', 'not_applied', 'cancelled', 'cancelled_unknown');

CREATE INDEX IF NOT EXISTS idx_commands_recovery_scan
    ON commands (seq)
    WHERE state NOT IN ('consumed', 'rejected', 'superseded', 'expired');
