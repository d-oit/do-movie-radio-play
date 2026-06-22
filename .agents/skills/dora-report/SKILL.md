---
name: dora-report
description: Generates DORA metrics and agent delivery reports from structured events.
---

# DORA Report

Aggregates events recorded by `metrics-reporter` to measure agent-assisted delivery performance.

## Metrics Tracked

- **Lead Time for Changes**: Time from task start to task finish.
- **Deployment Frequency**: Number of successful 'finished' events per day/week.
- **Change Failure Rate**: Ratio of failed 'finished' events to total 'finished' events.
- **Mean Time to Recovery (MTTR)**: Time between a failed 'finished' event and a subsequent successful 'finished' event for the same task/scope.
- **Human Intervention Rate**: Average number of human interventions per task.

## Usage

```bash
./.agents/skills/dora-report/generate.sh
```

## Output

Reports are written to `reports/dora-stats.json` and a human-readable summary to `analysis/dora-report.md`.
