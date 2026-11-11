#!/usr/bin/env bash
# Черновая цель ProoFee в testnet (SPEC 11, шаг 1).
# Собирает wasm и одной транзакцией от MASTER создаёт goal-<N>.<MASTER>: create-account,
# transfer 3 NEAR, deploy-contract, function-call new. Ключей у цели нет с рождения.
#
#   scripts/deploy-testnet.sh                          # черновая цель, N — первый свободный номер
#   scripts/deploy-testnet.sh scripts/goals/demo.json  # параметры цели из файла
#   N=5 scripts/deploy-testnet.sh                      # N задан вручную
#   MASTER=me.testnet scripts/deploy-testnet.sh
#   DRY_RUN=1 scripts/deploy-testnet.sh <файл>         # только показать аргументы new
#
# Формат файла — scripts/goals/demo.json: goal_title, goal_uri, immediate_bps,
# challenge_window_ms, milestones[{title, weight_bps, deadline_days}].
# deadline_days считается от момента запуска скрипта.
set -euo pipefail

NETWORK=testnet # только testnet, параметра нет
MASTER="${MASTER:-proofee-anton.testnet}"
# Субаккаунт MASTER: после удаления его может пересоздать только MASTER (AUDIT L-5).
VERIFIER="${VERIFIER:-verifier.$MASTER}"
DAY_MS=86400000

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
  [[ -f "$GOAL_FILE" ]] || { echo "Нет файла $GOAL_FILE" >&2; exit 1; }
  GOAL_SPEC=$(jq . "$GOAL_FILE")
else
  # Черновая цель: к goal_title ниже добавится номер, когда N станет известен.
  GOAL_SPEC=$(cat <<'EOF'
{
  "goal_title": "Draft goal",
  "goal_uri": "",
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
  echo "Сумма weight_bps = $WEIGHT_SUM, нужно 10000" >&2
  exit 1
fi

if [[ -n "$DRY_RUN" ]]; then
  N="${N:-X}" # в сухом прогоне сеть не трогаем
else
  echo "== Проверяю ключ $MASTER в связке ключей"
  if ! near --quiet account get-public-key from-keychain "$MASTER" network-config "$NETWORK" >/dev/null 2>&1; then
    echo "Нет ключа $MASTER в связке ключей. Импортируй его: near account import-account ..." >&2
    exit 1
  fi

  if [[ -z "${N:-}" ]]; then
    N=1
    while account_exists "goal-$N.$MASTER"; do N=$((N + 1)); done
  fi
  if account_exists "goal-$N.$MASTER"; then
    echo "goal-$N.$MASTER уже существует" >&2
    exit 1
  fi
fi
GOAL="goal-$N.$MASTER"

if [[ -z "$GOAL_FILE" ]]; then
  GOAL_SPEC=$(jq --arg t "Draft goal $N" '.goal_title = $t' <<<"$GOAL_SPEC")
fi

NOW_MS=$(($(date +%s) * 1000))
ARGS=$(jq --arg creator "$MASTER" --arg verifier "$VERIFIER" \
  --argjson now "$NOW_MS" --argjson day "$DAY_MS" '
  {
    creator: $creator,
    verifier: $verifier,
    goal_title,
    goal_uri,
    immediate_bps,
    challenge_window_ms,
    milestones: [.milestones[] | {
      title,
      weight_bps,
      deadline_ms: ($now + (.deadline_days * $day | floor))
    }]
  }' <<<"$GOAL_SPEC")

echo "== Цель $GOAL, аргументы new:"
echo "$ARGS"
jq -r '.milestones[] | "   \(.weight_bps) bps  до \(.deadline_ms / 1000 | floor | strflocaltime("%Y-%m-%d %H:%M"))  \(.title)"' <<<"$ARGS"
echo "   сумма весов: $WEIGHT_SUM"

if [[ -n "$DRY_RUN" ]]; then
  echo "== DRY_RUN: транзакция не отправлена"
  exit 0
fi

echo "== Собираю wasm"
(cd "$ROOT/contracts/goal" && cargo near build non-reproducible-wasm)

# Одна транзакция: если любое действие упадёт, откатятся все, и окна между деплоем и new
# нет. Подписант — MASTER, поэтому возврат неистраченного газа уходит ему, а не цели
# (на счёте цели он выглядел бы доходом).
echo "== Создаю $GOAL одной транзакцией от $MASTER"
near transaction construct-transaction "$MASTER" receiver-id "$GOAL" \
  add-action create-account \
  add-action transfer '3 NEAR' \
  add-action deploy-contract use-file "$WASM" without-init-call \
  add-action function-call new json-args "$ARGS" prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
  skip network-config "$NETWORK" sign-with-keychain send

echo "== Готово: $GOAL"
view "$GOAL" get_goal
view "$GOAL" get_balances
echo "== Ключи $GOAL (должно быть пусто)"
near account list-keys "$GOAL" network-config "$NETWORK" now
