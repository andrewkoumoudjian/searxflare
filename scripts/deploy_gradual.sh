#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <previous-version-id> <new-version-id> <new-version-percentage>" >&2
  exit 2
fi

previous_version=$1
new_version=$2
new_percentage=$3

if [[ ! $new_percentage =~ ^[0-9]+$ ]] || (( new_percentage < 1 || new_percentage > 100 )); then
  echo "new-version-percentage must be an integer from 1 through 100" >&2
  exit 2
fi

if [[ $previous_version == "$new_version" ]]; then
  echo "previous and new version IDs must differ" >&2
  exit 2
fi

previous_percentage=$((100 - new_percentage))
npx wrangler versions deploy \
  "${previous_version}@${previous_percentage}" \
  "${new_version}@${new_percentage}" \
  --message "Gradual searxflare rollout: ${new_percentage}% new" \
  --yes
