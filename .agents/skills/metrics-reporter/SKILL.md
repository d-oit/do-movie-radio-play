---
name: metrics-reporter
description: Records agent execution events for lead time, success rate, and human intervention tracking.
---

# Metrics Reporter

Records structured events during agent execution to enable DORA and productivity measurement.

## When to use

- At the start and end of a significant task or workflow.
- When human intervention is required.
- When an artifact is produced.

## Schema

Events are stored in `.agents/events/YYYY/MM/DD/` as individual JSON files.
Schema is defined in `schema/agent-event.schema.json`.

## Usage

### Recording a 'started' event

```bash
./.agents/skills/metrics-reporter/report.sh \
  --task-id "task-123" \
  --workflow-id "wf-456" \
  --skill "goap-agent" \
  --event-type "started"
```

### Recording a 'finished' event

```bash
./.agents/skills/metrics-reporter/report.sh \
  --task-id "task-123" \
  --workflow-id "wf-456" \
  --skill "goap-agent" \
  --event-type "finished" \
  --success true \
  --human-interventions 0 \
  --artifacts "reports/result.html"
```

## Environment Variables

- `AGENT_TASK_ID`: Optional default task ID.
- `AGENT_WORKFLOW_ID`: Optional default workflow ID.
