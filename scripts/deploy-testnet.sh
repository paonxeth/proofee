#!/usr/bin/env bash
# ProoFee goal in testnet (SPEC 11, step 1).
# Builds the wasm and creates goal-<N>.<MASTER> in one transaction from MASTER: create-account,
# transfer 3 NEAR, deploy-contract, function-call new. The goal has no keys from the start.
#
#   scripts/deploy-testnet.sh                          # draft goal, N is the first free number
#   scripts/deploy-testnet.sh scripts/goals/demo.json  # goal parameters from a file
#   N=5 scripts/deploy-testnet.sh                      # N set manually
#   MASTER=me.testnet scripts/deploy-testnet.sh
#   DRY_RUN=1 scripts/deploy-testnet.sh <file>         # only print the new() arguments
#
# File format (example: scripts/goals/demo.json): goal_title, spec_file, immediate_bps,
# challenge_window_ms, milestones[{title, weight_bps, deadline_days}].
# deadline_days counts from the moment the script runs.
# spec_file is the goal spec in this repo (example: specs/demo-goal.json). It must be committed
# and pushed: goal_uri becomes a raw GitHub link pinned to the commit, goal_spec_hash its sha256.
set -euo pipefail

NETWORK=testnet # testnet only, no parameter for it
MASTER="${MASTER:-proofee-anton.testnet}"
# A MASTER subaccount: if deleted, only MASTER can recreate it (AUDIT L-5).
VERIFIER="${VERIFIER:-verifier.$MASTER}"
DAY_MS=86400000
RAW_BASE="https://raw.githubusercontent.com/paonxeth/proofee"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM="$ROOT/contracts/goal/target/near/goal.wasm"

account_exists() {
  near --quiet account view-account-summary "$1" network-config "$NETWORK" now >/dev/null 2>&1
}

view() {
  near contract call-function as-read-only "$1" "$2" json-args '{}' network-config "$NETWORK" now
}

GOAL_FILE="${1:-}"
DRY_RUN="${DRY_RUN:-}"

if [[ -n "$GOAL_FILE" ]]; then
  [[ -f "$GOAL_FILE" ]] || { echo "No such file: $GOAL_FILE" >&2; exit 1; }
  GOAL_SPEC=$(jq . "$GOAL_FILE")
else
  # Draft goal: goal_title gets the number appended below, once N is known.
  GOAL_SPEC=$(cat <<'EOF'
{
  "goal_title": "Draft goal",
  "spec_file": "specs/draft.json",
  "immediate_bps": 5000,
  "challenge_window_ms": 120000,
  "milestones": [
    {"title": "Milestone 1", "weight_bps": 4000, "deadline_days": 3},
    {"title": "Milestone 2", "weight_bps": 3000, "deadline_days": 5},
    {"title": "Milestone 3", "weight_bps": 3000, "deadline_days": 7}
  ]
}
EOF
)
fi

WEIGHT_SUM=$(jq '[.milestones[].weight_bps] | add' <<<"$GOAL_SPEC")
if [[ "$WEIGHT_SUM" != 10000 ]]; then
  echo "weight_bps sum is $WEIGHT_SUM, must be 10000" >&2
  exit 1
fi

# The goal spec: committed, unchanged and pushed, so the pinned link serves exactly these bytes.
SPEC_FILE=$(jq -r '.spec_file // empty' <<<"$GOAL_SPEC")
[[ -n "$SPEC_FILE" ]] || { echo "spec_file is missing" >&2; exit 1; }
git -C "$ROOT" ls-files --error-unmatch -- "$SPEC_FILE" >/dev/null 2>&1 \
  || { echo "$SPEC_FILE is not committed" >&2; exit 1; }
git -C "$ROOT" diff --quiet HEAD -- "$SPEC_FILE" \
  || { echo "$SPEC_FILE has uncommitted changes" >&2; exit 1; }
SPEC_COMMIT=$(git -C "$ROOT" log -1 --format=%H -- "$SPEC_FILE")
if [[ -z "$(git -C "$ROOT" branch -r --contains "$SPEC_COMMIT")" ]]; then
  echo "$SPEC_FILE: commit $SPEC_COMMIT is not pushed" >&2
  exit 1
fi
GOAL_URI="$RAW_BASE/$SPEC_COMMIT/$SPEC_FILE"
GOAL_SPEC_HASH=$(git -C "$ROOT" show "$SPEC_COMMIT:$SPEC_FILE" | shasum -a 256 | cut -d' ' -f1)

if [[ -n "$DRY_RUN" ]]; then
  N="${N:-X}" # a dry run does not touch the network
else
  echo "== Checking the $MASTER key in the keychain"
  if ! near --quiet account get-public-key from-keychain "$MASTER" network-config "$NETWORK" >/dev/null 2>&1; then
    echo "No $MASTER key in the keychain. Import it: near account import-account ..." >&2
    exit 1
  fi

  if [[ -z "${N:-}" ]]; then
    N=1
    while account_exists "goal-$N.$MASTER"; do N=$((N + 1)); done
  fi
  if account_exists "goal-$N.$MASTER"; then
    echo "goal-$N.$MASTER already exists" >&2
    exit 1
  fi
fi
GOAL="goal-$N.$MASTER"

if [[ -z "$GOAL_FILE" ]]; then
  GOAL_SPEC=$(jq --arg t "Draft goal $N" '.goal_title = $t' <<<"$GOAL_SPEC")
fi

NOW_MS=$(($(date +%s) * 1000))
ARGS=$(jq --arg creator "$MASTER" --arg verifier "$VERIFIER" \
  --arg uri "$GOAL_URI" --arg hash "$GOAL_SPEC_HASH" \
  --argjson now "$NOW_MS" --argjson day "$DAY_MS" '
  {
    creator: $creator,
    verifier: $verifier,
    goal_title,
    goal_uri: $uri,
    goal_spec_hash: $hash,
    immediate_bps,
    challenge_window_ms,
    milestones: [.milestones[] | {
      title,
      weight_bps,
      deadline_ms: ($now + (.deadline_days * $day | floor))
    }]
  }' <<<"$GOAL_SPEC")

echo "== Goal $GOAL, new() arguments:"
echo "$ARGS"
jq -r '.milestones[] | "   \(.weight_bps) bps  until \(.deadline_ms / 1000 | floor | strflocaltime("%Y-%m-%d %H:%M"))  \(.title)"' <<<"$ARGS"
echo "   weight sum: $WEIGHT_SUM"
echo "   spec: $SPEC_FILE at $SPEC_COMMIT, sha256 $GOAL_SPEC_HASH"

if [[ -n "$DRY_RUN" ]]; then
  echo "== DRY_RUN: transaction not sent"
  exit 0
fi

echo "== Building wasm"
(cd "$ROOT/contracts/goal" && cargo near build non-reproducible-wasm)

# One transaction: if any action fails, all of them roll back, and there is no window
# between deploy and new. MASTER signs, so the unused gas refund goes to MASTER, not to the goal
# (on the goal's account it would look like income).
echo "== Creating $GOAL in one transaction from $MASTER"
near transaction construct-transaction "$MASTER" receiver-id "$GOAL" \
  add-action create-account \
  add-action transfer '3 NEAR' \
  add-action deploy-contract use-file "$WASM" without-init-call \
  add-action function-call new json-args "$ARGS" prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
  skip network-config "$NETWORK" sign-with-keychain send

echo "== Done: $GOAL"
view "$GOAL" get_goal
view "$GOAL" get_balances
echo "== Keys of $GOAL (must be empty)"
near account list-keys "$GOAL" network-config "$NETWORK" now
