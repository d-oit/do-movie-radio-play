#!/usr/bin/env bash
set -euo pipefail

bash scripts/quality_gate.sh && git add -A && git commit "$@"

# Record event if metrics-reporter is available
if [ -f ".agents/skills/metrics-reporter/report.sh" ]; then
  # Try to use AGENT_TASK_ID, fallback to short commit SHA
  TASK_ID="${AGENT_TASK_ID:-$(git rev-parse --short HEAD)}"
  ./.agents/skills/metrics-reporter/report.sh \
    --task-id "$TASK_ID" \
    --workflow-id "${AGENT_WORKFLOW_ID:-$TASK_ID}" \
    --skill "atomic-commit" \
    --event-type "finished" \
    --success true
fi
