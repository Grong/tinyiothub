-- Isolated simulation composition (X2/XD3): execution mode is explicit on
-- Harness Tasks/Runs, and the database itself rejects any simulation write
-- that could create an Effect, Decision, or production wake.
ALTER TABLE agent_tasks ADD COLUMN execution_mode TEXT NOT NULL DEFAULT 'production';
ALTER TABLE agent_runs ADD COLUMN execution_mode TEXT NOT NULL DEFAULT 'production';

CREATE INDEX idx_agent_tasks_execution_mode
    ON agent_tasks (execution_mode, id);
CREATE INDEX idx_agent_runs_execution_mode
    ON agent_runs (execution_mode, task_id);

CREATE TRIGGER effect_intents_reject_simulation_insert
BEFORE INSERT ON effect_intents
WHEN (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot create effects');
END;

CREATE TRIGGER effect_intents_reject_simulation_update
BEFORE UPDATE ON effect_intents
WHEN (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot update effects');
END;

CREATE TRIGGER decisions_reject_simulation_insert
BEFORE INSERT ON decisions
WHEN (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot create decisions');
END;

CREATE TRIGGER decisions_reject_simulation_update
BEFORE UPDATE ON decisions
WHEN (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot update decisions');
END;

CREATE TRIGGER task_wake_outbox_reject_simulation
BEFORE INSERT ON task_wake_outbox
WHEN (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot enqueue production wakes');
END;

-- Reachable dispatch/occupancy surfaces outside the three core tables above.
CREATE TRIGGER workflow_occurrences_reject_simulation
BEFORE INSERT ON workflow_occurrences
WHEN (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot create workflow occurrences');
END;

CREATE TRIGGER device_jobs_reject_simulation_insert
BEFORE INSERT ON device_jobs
WHEN NEW.task_id IS NOT NULL
 AND (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot create device jobs');
END;

CREATE TRIGGER device_jobs_reject_simulation_update
BEFORE UPDATE ON device_jobs
WHEN NEW.task_id IS NOT NULL
 AND (SELECT execution_mode FROM agent_tasks WHERE id = NEW.task_id) = 'simulation'
BEGIN
    SELECT RAISE(ABORT, 'SimulationDispatchBlocked: simulation cannot update device jobs');
END;
