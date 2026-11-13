# План: подключение contracts/goal к Bond Treasury v0.2.0

Согласовано 2026-11-13. Ветка `feat/goal-treasury`.

## Контекст

Сейчас challenge бесплатный: любой может заблокировать этап на 24 ч. Treasury v0.2.0 (`bond-treasury/`) уже развёрнут как отдельный контракт с бондом, slash в buyback reserve и нейтральным возвратом по таймауту. Goal должен научиться с ним работать: оркестрацию переносим из `bond-treasury/contracts/mock_goal/src/lib.rs`, заменяя admin на verifier/челленджера. Награды (fund_reward/pay_reward) не делаем, учёт sync/claim не трогаем.

Найдено при чтении:
- Treasury: `register_intent` проверяет `B = clamp(F/5, 1N, 5N)`, `R = min(F/10, B/2, 2N)` и `now < deposit_until < bind_until < refund_after`; `settle_bond`/`authorize_decision` работают только при `now < refund_after`; `refund_expired` (кто угодно) — Deposited после `bind_until` или Bound после `refund_after` → NeutralRefund. Повтор settle с тем же id — no-op, конфликт — panic.
- Treasury `new` требует суффикс `.testnet` → в песочнице нужен genesis-аккаунт `testnet` (`SandboxConfig.additional_accounts`, near-sandbox 0.4).
- `release/bond_treasury.wasm` совпадает с SHA256SUMS (5dfb0699…).
- Тестов сейчас 80 в lib.rs + 3 в math.rs = 83 юнит, 3 интеграционных.

## 1. Состояние

```rust
pub struct Goal { ...; treasury: Option<AccountId> }          // задаётся в new, не меняется

pub struct Milestone { ...;
    challenge_attempts: u32,              // счётчик заявок → уникальные proof_id/case_id
    case: Option<ChallengeCase>,          // текущая (последняя) заявка с бондом
}

pub struct ChallengeCase {
    proof_id: String,      // "{index}:{proof_hash}:{attempt}"
    case_id: String,       // "m{index}-{attempt}"  (≤121 байт)
    payer: AccountId,      // челленджер
    reason: String,        // хранится ТОЛЬКО здесь; в режиме Treasury milestone.challenge_reason = None
                           // (поле этапа используется только бесплатным challenge)
    fee_snapshot: U128,    // F
    amount: U128,          // B
    reward_cap: U128,      // R (только для Treasury, наград не платим)
    deposit_until_ns, bind_until_ns, refund_after_ns: u64,
    escrow_id: Option<String>,
    stage: CaseStage,
    decision: Option<Decision>,
    nonce: u64,            // защита колбэков от устаревших результатов (в Mock — attempt)
}

enum CaseStage { RegisterPending, RegisterRetryable, Ready, VerifyPending, BindPending,
    BindRetryable, Bound, Qualified, DecisionPending, DecisionRetryable,
    SettlementPending, SettlementRetryable, Settled, TimeoutRequested, Orphaned }
```

Сроки (константы, при `prepare_challenge` в момент `t0`, всё в ns, `ms × 1_000_000` через `checked_mul`):
- `deposit_until = t0 + 10 мин`, `bind_until = t0 + 20 мин`,
- `refund_after = bind_until + RESOLVE_TIMEOUT_MS (24 ч)`.
- Для этапа в режиме Treasury дедлайн решения = `refund_after` (а не `challenged_at + 24 ч`). Так у verifier всегда ≥ 24 ч после привязки, и граница Goal совпадает с границей Treasury: до неё работают `qualify/arbitrate`, после — только таймаут.

**F** = `L × weight_bps / 10000`, где `L` — locked из SPEC 5.2 по `total_received` после `sync()` в `prepare_challenge`. Если идёт выплата (`payout_in_flight`), sync пропускается и берётся уже учтённое — это и есть «учтённые фисы на момент спора». B и R — ровно формулы Treasury (иначе `ECONOMICS`).

## 1a. Резерв хранения (обязательно)

Проблема: case на каждом этапе (reason до 280 символов, id, сроки) — рост состояния за счёт контракта. `claim` оставляет только `STORAGE_RESERVE_BYTES`; если рост больше, после полного claim `prepare_challenge` упадёт с `LackBalanceForState`, и креатор фактически отключит споры.

Что делаю:
- Рост ограничен сверху: на этап хранится одна (последняя) заявка, новая перезаписывает старую; reason хранится один раз (в case); `challenger` этапа заполняется только при bind.
- Замер в песочнице (шаг 4, первый тест): goal с 5 этапами, все строки на пределе (title 100, uri 256, reason 280 символов, многобайтные UTF-8 — лимиты в символах, байт больше), Treasury с максимальными F → `storage_usage` до и после: prepare на всех 5 этапах → bind → отказ квалификации → новая заявка на каждом этапе. Максимум роста фиксирую в тесте.
- `claim` уже оставляет `storage_usage + STORAGE_RESERVE_BYTES` (`storage_cost_with_reserve`), т.е. текущее состояние и код покрыты. Резерв покрывает только рост сверх этого: `STORAGE_RESERVE_BYTES ≥ 1,5 × измеренный рост` (округлённо, не меньше текущих 16000). Минимальный депозит в `new` остаётся `storage + 2 × STORAGE_RESERVE_BYTES`. Если окажется, что ограничения reason надо ужать — сначала спрошу.
- После сборки проверяю, что 3 NEAR в `scripts/deploy-testnet.sh` покрывают новый минимум (wasm вырастет); если нет — поднимаю и пишу, сколько.
- Тесты: (а) песочница — creator делает полный `claim`, после этого весь худший сценарий (5 prepare → bind → отказ → 5 новых prepare) проходит без `LackBalanceForState`, и `рост ≤ STORAGE_RESERVE_BYTES`; (б) юнит — `new` отклоняет депозит на 1 yocto меньше нового минимума и принимает ровно минимум (обновлённый `MIN_DEPOSIT_YOCTO`).
- SPEC §3 и §5.3: новое число байт и ≈NEAR, обоснование замером.

## 1b. Известное ограничение (код не чиним, только SPEC + docs/AUDIT.md)

`prepare_challenge` стоит только газ и держит этап до `bind_until` без бонда. Креатор с другого аккаунта может каждые 20 минут подавать пустую заявку и не пускать настоящих челленджеров всё окно. Плюс каждая заявка навсегда занимает место в Treasury (`register_intent` ничего не удаляет, хранение платит Treasury). Фикс после хакатона: несколько заявок параллельно, выигрывает первая привязанная. Записываю в SPEC (раздел ограничений) и в AUDIT.md как открытую находку — в коммите шага 2.

## 2. Методы

`new(..., treasury: Option<AccountId>)`: при Some — `treasury ≠ self, creator, verifier`. Отсутствующее поле в JSON → None, поэтому старые интеграционные тесты и скрипты не меняются.

При `treasury = None` всё как сейчас; новые методы паникуют `"No treasury"`.
При `treasury = Some`: `challenge` → `"Bond required"`, `resolve` → `"Use arbitrate"`.

| Метод | Кто | Условие | Действие |
|---|---|---|---|
| `prepare_challenge(index, reason)` | кто угодно, кроме creator и verifier | Submitted, окно открыто, нет активной заявки (нет `case` или она Orphaned/Settled-после-отказа, или не привязана и `now ≥ bind_until`) | sync, F/B/R, запись case (RegisterPending) → `register_intent` → `on_registered` |
| `retry_registration(index)` | payer | RegisterRetryable | тот же intent (Treasury идемпотентен) |
| *(челленджер сам)* `treasury.deposit_bond(escrow_id)` ровно B | payer | | NEAR бонда идут в Treasury, goal их не видит |
| `confirm_escrow(index)` | payer | Ready / BindRetryable | `get_bond` → `on_verified` (сверка goal/proof/case/payer/amount/сроков, статус Deposited\|Bound) → `bind_bond` → `on_bound`: этап **Challenged**, `challenger/reason/challenged_at` заполняются, событие `proof_challenged` |
| `qualify(index, qualified)` | verifier | Challenged, case Bound, `now < refund_after` | true → Qualified (без вызова Treasury). false → этап **Submitted** (поля challenger очищаются), decision QualificationRejected → `authorize_decision` → `on_decided` → `settle_bond(REFUND, "bond:{case_id}")` → `on_settled` |
| `arbitrate(index, valid)` | verifier | Challenged, case Qualified, `now < refund_after` | valid → этап **Accepted**, ProofValid → SLASH. invalid → этап **Rejected**, ProofInvalidFinal → REFUND. Та же цепочка decided→settled |
| `retry_decision(index)` / `retry_settlement(index)` | кто угодно | DecisionRetryable / SettlementRetryable | повтор с теми же аргументами (decision и settlement_id неизменны) |
| `resolve_timeout(index)` | кто угодно | Challenged, case Bound/Qualified, `now ≥ refund_after` | этап **Accepted**, stage TimeoutRequested → `treasury.refund_expired` → `on_timeout_refund` (только событие) |
| `finalize(index)` | как сейчас | + не блокируется заявкой, если она не привязана и `now ≥ bind_until` | без изменений иначе |

Статус этапа меняется сразу в методе verifier/таймаута («state first»), а цепочка в Treasury — только про бонд. Это безопасно: если `authorize_decision` дойдёт до Treasury уже после `refund_after`, бонд вернётся нейтрально, а статус этапа тот же, что дал бы таймаут (valid→Accepted) или решение (invalid→Rejected, бонд и так возвращается).

## 3. Асинхронные цепочки и сбои колбэков

Все колбэки `#[private]`, принимают `(index, nonce)`, **никогда не паникуют** (как `on_claim_transfer`): при несовпадении stage/nonce — `return false`. Перед каждым вызовом: stage = *Pending, `nonce += 1`. Газ — как в Mock (register 35+15, get_bond 15 + on_verified 80, bind 35+15, authorize 35 + on_decided 85, settle 60+15, refund_expired 60+15); пользователю 300 TGas.

| Сбой | Что происходит |
|---|---|
| `register_intent` упал | RegisterRetryable → payer зовёт `retry_registration`; либо через 20 мин можно подать новую заявку |
| бонд не внесён / не подтверждён | этап остаётся Submitted; после `bind_until` finalize и новая заявка разрешены; внесённый бонд кто угодно возвращает `treasury.refund_expired` |
| `get_bond` не сошёлся / `bind_bond` упал | BindRetryable → payer повторяет `confirm_escrow` до `bind_until` |
| `on_bound` пришёл, а этап уже не Submitted (гонка с finalize на границе `bind_until`) | stage Orphaned, этап не трогаем; бонд Bound → нейтральный возврат после `refund_after` |
| `authorize_decision` упал | DecisionRetryable → `retry_decision` (кто угодно) до `refund_after` |
| `settle_bond` упал | SettlementRetryable → `retry_settlement`; повтор с тем же id в Treasury — no-op, двойной выплаты нет |
| ретраи опоздали | кто угодно вызывает `treasury.refund_expired` → NeutralRefund |
| `refund_expired` из goal упал (например, уже вызван напрямую) | только событие; бонд всё равно возвращается через Treasury |

Goal не переводит NEAR и не прикладывает deposit ни к одному вызову Treasury → `sync`, `claim`, `last_balance` не меняются. Settled в Goal = «инструкция принята»; фактический Refunded/Slashed смотрим через `treasury.get_bond` (UI).

События: `challenge_prepared` {index, case_id, payer, amount}, `proof_challenged` (при bind), `challenge_qualified` {index, qualified}, `bond_settlement` {index, decision, stage}.

## 4. Порядок работы и коммиты

1. `docs/TREASURY_INTEGRATION_PLAN.md` (этот план) — коммит.
2. SPEC: §2 (бонд теперь опционально), §3 параметр `treasury`, §4 состояние, §6 переходы, §7 методы/ошибки, §8 события, §10 тесты; DECISIONS: запись 2026-11-13 (F, сроки 10/10 мин + 24 ч, отказ квалификации → Submitted, награды нет). — коммит.
3. Код в `contracts/goal/src/lib.rs` (+ `treasury.rs` для case/колбэков, чтобы lib.rs не разрастался) и юнит-тесты; существующие тесты меняются только механически (добавить `None` в 4 прямых вызова `Goal::new` и хелпер). — коммит.
4. `contracts/goal/tests/treasury.rs` — песочница с настоящим `bond-treasury/release/bond_treasury.wasm`: сначала замер хранения и скорость `fast_forward`, затем подъём `STORAGE_RESERVE_BYTES` (+ SPEC, + юнит-тест депозита) отдельным коммитом, затем сценарии. — коммиты.

В testnet ничего не деплою. В 19:00 — контрольная точка: если песочница не зелёная, коммичу что есть и пишу, что осталось.

## 5. Тесты

Юнит (новые, через `VMContextBuilder` + `PromiseResult`, по образцу тестов Mock): `new` без treasury = старое поведение; `treasury == creator` отклоняется; `challenge`/`resolve` в режиме Treasury паникуют; расчёт F/B/R (F=0 → B=1N,R=0; большой F → потолок 5N/2N); prepare от creator/verifier запрещён; повторная prepare при активной заявке запрещена, после `bind_until` разрешена; неподтверждённая заявка не блокирует finalize после `bind_until` и блокирует до; каждый колбэк: успех, сбой → Retryable, устаревший nonce → no-op; `on_bound` после finalize → Orphaned; qualify/arbitrate только verifier и только до `refund_after`; resolve_timeout до/после `refund_after`; `sync` не меняет `total_received` на протяжении сценария.

Песочница (`tests/treasury.rs`): genesis `testnet` → `treasury.testnet` (new + register_goal + fund_storage), goal с `treasury`, fees 10 NEAR, proof:
1. отказ квалификации → бонд Refunded, баланс челленджера +B, этап Submitted;
2. valid → Slashed, `get_buyback_balance(goal) = B`, этап Accepted;
3. invalid → Refunded, этап Rejected;
4. таймаут: `fast_forward` за `refund_after` → `resolve_timeout` → Accepted, decision NeutralRefund, челленджер +B;
5. повторное урегулирование: `retry_settlement` после Settled → ошибка; прямой `settle_bond`/`refund_expired` в Treasury → ошибка; баланс челленджера не изменился второй раз;
6. посторонний: `qualify`/`arbitrate` → `"Only verifier"`; `confirm_escrow` не от payer → `"Only payer"`.
Во всех сценариях: `total_received` goal не вырос на B.

7. резерв хранения (см. 1a): полный claim → худший сценарий проходит, рост ≤ резерва.

Таймаут: ~26 ч в песочнице (~310 тыс. блоков `fast_forward`). Проверяю первым делом. Если слишком медленно — feature `short-timeouts` в `Cargo.toml` goal: под `#[cfg(feature = "short-timeouts")]` укорочены `RESOLVE_TIMEOUT_MS` и окна депозита/привязки (например, 5 мин / 1 / 1 мин); песочничные тесты Treasury собирают wasm через `cargo_near_build` с этой feature. Релизный wasm (`cargo near build non-reproducible-wasm`, контейнерная команда в Cargo.toml) — без feature, 24 ч. Старые 3 интеграционных теста продолжают использовать обычную сборку. Плюс юнит-тест, что без feature константа = 86_400_000.

## Проверка

- `cd contracts/goal && cargo test` — 83 старых юнит + новые, 3 старых интеграционных + новые песочничные, всё зелёное.
- `cargo near build non-reproducible-wasm` собирается.
- `git log` в `feat/goal-treasury` — 4 коммита по шагам; `main` не тронут.
