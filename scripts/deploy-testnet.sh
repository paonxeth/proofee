#!/usr/bin/env bash
# Черновая цель ProoFee в testnet (SPEC 11, шаг 1).
# Собирает wasm, создаёт субаккаунт goal-<N>.<MASTER> с 3 NEAR, деплоит и вызывает new.
# Full access ключ цели НЕ удаляется: это черновик.
#
#   scripts/deploy-testnet.sh            # N — первый свободный номер
#   N=5 scripts/deploy-testnet.sh        # N задан вручную
#   MASTER=me.testnet scripts/deploy-testnet.sh
set -euo pipefail

NETWORK=testnet # только testnet, параметра нет
MASTER="${MASTER:-proofee-anton.testnet}"
VERIFIER="${VERIFIER:-proofee-verifier.testnet}"
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

echo "== Создаю $GOAL с 3 NEAR"
near account create-account fund-myself "$GOAL" '3 NEAR' \
  autogenerate-new-keypair save-to-keychain \
  sign-as "$MASTER" network-config "$NETWORK" sign-with-keychain send

echo "== Деплою код"
near contract deploy "$GOAL" use-file "$WASM" without-init-call \
  network-config "$NETWORK" sign-with-keychain send

# new подписывает MASTER, а не сам аккаунт цели: возврат неистраченного газа приходит
# подписанту уже после new, и на счёте цели он выглядел бы доходом.
echo "== Вызываю new от $MASTER"
near contract call-function as-transaction "$GOAL" new json-args "$ARGS" \
  prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
  sign-as "$MASTER" network-config "$NETWORK" sign-with-keychain send

# Между деплоем и new цель мог инициализировать кто-то другой: тогда наш new упал бы
# выше. Проверяем, что параметры наши.
GOAL_JSON=$(view "$GOAL" get_goal)
if ! grep -q "\"creator\": \"$MASTER\"" <<<"$GOAL_JSON" ||
  ! grep -q "\"verifier\": \"$VERIFIER\"" <<<"$GOAL_JSON"; then
  echo "Цель инициализирована не нашими параметрами:" >&2
  echo "$GOAL_JSON" >&2
  exit 1
fi

echo "== Готово: $GOAL"
echo "$GOAL_JSON"
view "$GOAL" get_balances
