#!/usr/bin/env bash
set -euo pipefail

# DORA Report Generator
# Aggregates events from .agents/events/

EVENTS_DIR=".agents/events"
OUTPUT_JSON="reports/dora-stats.json"
OUTPUT_MD="analysis/dora-report.md"

mkdir -p reports analysis

echo "Analyzing events in $EVENTS_DIR..."

ALL_EVENTS=$(find "$EVENTS_DIR" -name "*.json" | xargs cat | jq -s '.')

if [ "$ALL_EVENTS" == "[]" ]; then
  echo "No events found."
  exit 0
fi

# Calculate metrics using jq
# This is a simplified calculation for the POC
METRICS=$(echo "$ALL_EVENTS" | jq '
  def to_epoch: . + "Z" | fromdateiso8601;

  group_by(.task_id) | map({
    task_id: .[0].task_id,
    started: (map(select(.event_type == "started")) | sort_by(.timestamp) | .[0].timestamp),
    finished: (map(select(.event_type == "finished")) | sort_by(.timestamp) | .[-1].timestamp),
    success: (map(select(.event_type == "finished")) | sort_by(.timestamp) | .[-1].success),
    interventions: (map(.human_interventions // 0) | add)
  }) as $tasks
  | $tasks | map(select(.started != null and .finished != null)) as $completed
  | {
      total_tasks: ($tasks | length),
      completed_tasks: ($completed | length),
      success_rate: (if ($completed | length) > 0 then ($completed | map(select(.success == true)) | length) / ($completed | length) else 0 end),
      avg_lead_time_sec: (if ($completed | length) > 0 then ($completed | map((.finished | fromdateiso8601) - (.started | fromdateiso8601)) | add) / ($completed | length) else 0 end),
      total_interventions: ($tasks | map(.interventions) | add),
      avg_interventions_per_task: (if ($tasks | length) > 0 then ($tasks | map(.interventions) | add) / ($tasks | length) else 0 end)
    }
')

echo "$METRICS" > "$OUTPUT_JSON"

# Generate Markdown report
TOTAL_TASKS=$(echo "$METRICS" | jq '.total_tasks')
COMPLETED_TASKS=$(echo "$METRICS" | jq '.completed_tasks')
SUCCESS_RATE=$(echo "$METRICS" | jq '.success_rate * 100')
AVG_LEAD_TIME=$(echo "$METRICS" | jq '.avg_lead_time_sec / 60')
AVG_INTERVENTIONS=$(echo "$METRICS" | jq '.avg_interventions_per_task')

cat <<EOF > "$OUTPUT_MD"
# DORA and Agent Delivery Report
Generated on: $(date)

## Summary Metrics
- **Total Tasks**: $TOTAL_TASKS
- **Completed Tasks**: $COMPLETED_TASKS
- **Success Rate**: ${SUCCESS_RATE}%
- **Avg. Lead Time (min)**: ${AVG_LEAD_TIME}
- **Avg. Human Interventions**: $AVG_INTERVENTIONS

## Interpretation
- **Deployment Frequency**: Based on $COMPLETED_TASKS completions in the tracked period.
- **Change Failure Rate**: $((100 - ${SUCCESS_RATE%.*}))% (based on success rate).
EOF

echo "Reports generated: $OUTPUT_JSON, $OUTPUT_MD"
