# Automation And Workflows

## Purpose

This subsystem owns scheduled execution, compiled automation plans, run tracking, and persistent multi-step workflows.

## Primary Code

- `src/scheduler/cron.rs`
- `src/scheduler/automations.rs`
- `src/workflows/engine.rs`
- `src/workflows/db.rs`
- `src/workflows/mod.rs`

## Scheduler Model

`CronScheduler` is the single scheduler loop. It checks both legacy cron jobs and richer automations every 30 seconds.

Supported stored schedule formats:

- `cron:<expr>`
- `every:<seconds>`
- one-shot `at:` style exists for legacy cron jobs

`AutomationSchedule` normalizes the automation schedule formats and computes next run times.

## Automations

Automations are richer than simple cron jobs. They carry:

- prompt
- schedule
- delivery target
- trigger semantics
- compiled plan metadata
- dependency metadata
- validation errors
- execution history

At runtime, the scheduler can recompile an automation's plan from the saved prompt and current config, then mark it `active` or `invalid_config`.

## Run-Now Behavior

Automations are not executed by replaying a "create automation" utterance. `automations.rs` contains normalization logic that extracts the real task and wraps it in an execution-mode prompt so "run now" means "execute the job", not "create another automation".

## Workflow Integration

If an automation has workflow steps, the scheduler can create a workflow instead of just sending a prompt to the agent.

`WorkflowEngine` owns:

- workflow creation
- step persistence
- sequential execution
- inter-step context
- pause for approval
- resume
- retries
- cancel
- restart
- delete

Execution is delegated step-by-step through `AgentLoop::process_message()`.

## Persistence

SQLite stores:

- cron jobs
- automations
- automation runs
- workflow definitions
- workflow steps

This is why automations and workflows survive restarts.

## Failure Modes And Limits

- cron and automations share one scheduler loop, so regressions here affect both
- invalid MCP/skill dependencies can invalidate existing automations
- workflow execution is sequential today, not parallel fan-out/fan-in orchestration

## Change Checklist

Update this document when you change:

- schedule formats
- automation compilation/validation rules
- run-now normalization logic
- workflow lifecycle or persistence rules
