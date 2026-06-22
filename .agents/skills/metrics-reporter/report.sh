#!/usr/bin/env bash
set -euo pipefail

# Metrics Reporter Script
# Usage: ./report.sh --task-id ID --workflow-id ID --skill SKILL --event-type [started|finished] [--success true/false] [--human-interventions N] [--artifacts "file1,file2"]

TASK_ID=""
WORKFLOW_ID=""
SKILL=""
EVENT_TYPE=""
SUCCESS="null"
HUMAN_INTERVENTIONS=0
ARTIFACTS="[]"

while [[ $# -gt 0 ]]; do
  case $1 in
    --task-id) TASK_ID="$2"; shift 2 ;;
    --workflow-id) WORKFLOW_ID="$2"; shift 2 ;;
    --skill) SKILL="$2"; shift 2 ;;
    --event-type) EVENT_TYPE="$2"; shift 2 ;;
    --success) SUCCESS="$2"; shift 2 ;;
    --human-interventions) HUMAN_INTERVENTIONS="$2"; shift 2 ;;
    --artifacts)
      IFS=',' read -r -a array <<< "$2"
      ARTIFACTS="["
      for i in "${!array[@]}"; do
        ARTIFACTS+="\"${array[$i]}\""
        if [ $i -lt $((${#array[@]} - 1)) ]; then ARTIFACTS+=","; fi
      done
      ARTIFACTS+="]"
      shift 2 ;;
    *) shift ;;
  esac
done

if [ -z "$TASK_ID" ] || [ -z "$SKILL" ] || [ -z "$EVENT_TYPE" ]; then
  echo "Usage: $0 --task-id ID --skill SKILL --event-type [started|finished] [options]"
  exit 1
fi

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
GIT_SHA=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
DATE_PATH=$(date -u +"%Y/%m/%d")

mkdir -p ".agents/events/$DATE_PATH"
EVENT_FILE=".agents/events/$DATE_PATH/${TIMESTAMP}_${EVENT_TYPE}_${TASK_ID}.json"

cat <<EOF > "$EVENT_FILE"
{
  "task_id": "$TASK_ID",
  "workflow_id": "${WORKFLOW_ID:-$TASK_ID}",
  "skill": "$SKILL",
  "event_type": "$EVENT_TYPE",
  "timestamp": "$TIMESTAMP",
  "success": $SUCCESS,
  "human_interventions": $HUMAN_INTERVENTIONS,
  "git_sha": "$GIT_SHA",
  "branch": "$BRANCH",
  "artifacts": $ARTIFACTS
}
EOF

echo "Event recorded to $EVENT_FILE"
