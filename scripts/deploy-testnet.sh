#!/usr/bin/env bash
# Черновая цель ProoFee в testnet (SPEC 11, шаг 1).
# Собирает wasm и одной транзакцией от MASTER создаёт goal-<N>.<MASTER>: create-account,
# transfer 3 NEAR, deploy-contract, function-call new. Ключей у цели нет с рождения.
#
#   scripts/deploy-testnet.sh            # N — первый свободный номер
#   N=5 scripts/deploy-testnet.sh        # N задан вручную
#   MASTER=me.testnet scripts/deploy-testnet.sh
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

echo "== Проверяю ключ $MASTER в связке ключей"
if ! near --quiet account get-public-key from-keychain "$MASTER" network-config "$NETWORK" >/dev/null 2>&1; then
  echo "Нет ключа $MASTER в связке ключей. Импортируй его: near account import-account ..." >&2
  exit 1
fi

echo "== Собираю wasm"
(cd "$ROOT/contracts/goal" && cargo near build non-reproducible-wasm)

if [[ -z "${N:-}" ]]; then
  N=1
  while account_exists "goal-$N.$MASTER"; do N=$((N + 1)); done
fi
GOAL="goal-$N.$MASTER"
if account_exists "$GOAL"; then
  echo "$GOAL уже существует" >&2
  exit 1
fi

NOW_MS=$(($(date +%s) * 1000))
ARGS=$(cat <<EOF
{
  "creator": "$MASTER",
  "verifier": "$VERIFIER",
  "goal_title": "Draft goal $N",
  "goal_uri": "",
  "immediate_bps": 5000,
  "challenge_window_ms": 120000,
  "milestones": [
    {"title": "Milestone 1", "weight_bps": 4000, "deadline_ms": $((NOW_MS + 3 * DAY_MS))},
    {"title": "Milestone 2", "weight_bps": 3000, "deadline_ms": $((NOW_MS + 5 * DAY_MS))},
    {"title": "Milestone 3", "weight_bps": 3000, "deadline_ms": $((NOW_MS + 7 * DAY_MS))}
  ]
}
EOF
)

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
