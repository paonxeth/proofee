# ProoFee — спецификация контракта цели (MVP)

Версия 1.0 · 8 ноября 2026 · источник правды для `contracts/goal`.
Любое изменение поведения контракта начинается с правки этого файла.

## 1. Что это

Один экземпляр контракта = одна цель. Адрес контракта ставится получателем creator fees
токена (поле `fee_to` в Nearly). Контракт принимает NEAR, ведёт учёт и платит креатору:
часть сразу, остальное — по мере принятых этапов. Доля этапов, которые отклонены или
провалены, остаётся в контракте навсегда.

MVP работает только в testnet. Фисы в демо имитируются обычными переводами NEAR на
контракт. Для контракта это то же самое, что выплата от Nearly: NEAR-часть фисов Nearly
отправляет обычными переводами (проверено в mainnet на запуске #3680).

## 2. Что не входит в MVP

- Приём фисов токенами (NEP-141): ни сам мемкоин, ни wNEAR контракт не учитывает.
- Залог за спор.
- Переподача пруфа после отказа.
- Комитет 3 из 5 — в MVP решает один верификатор. Комитет позже можно сделать без
  изменения контракта: верификатором ставится мультисиг или DAO-аккаунт.
- Порядок этапов — этапы независимы друг от друга.
- Обновление кода: после деплоя full access ключ удаляется.

## 3. Параметры (задаются в `new`, потом не меняются)

| Поле | Тип | Проверка в `new` | Демо |
| --- | --- | --- | --- |
| `creator` | AccountId | не равен `verifier` | аккаунт креатора |
| `verifier` | AccountId | — | аккаунт команды |
| `goal_title` | String | 1–100 символов | |
| `goal_uri` | String | до 256 символов | ссылка на описание цели |
| `immediate_bps` | u16 | ≤ 10000 | 5000 (50%) |
| `challenge_window_ms` | u64 | > 0 | 120000 (2 минуты); по концепции 172800000 (48 ч) |
| `milestones` | Vec<MilestoneInput> | 1–5 этапов, у каждого `weight_bps` > 0, сумма весов = 10000, `deadline_ms` > текущего времени, `title` 1–100 символов | |

`MilestoneInput { title: String, weight_bps: u16, deadline_ms: u64 }`

## 4. Состояние

```rust
pub struct Goal {
    creator: AccountId,
    verifier: AccountId,
    token: Option<AccountId>,      // только для информации, ставится один раз
    goal_title: String,
    goal_uri: String,
    immediate_bps: u16,
    challenge_window_ms: u64,
    milestones: Vec<Milestone>,
    total_received: NearToken,     // весь доход за всё время
    claimed: NearToken,            // уже выплачено креатору
    last_balance: NearToken,       // баланс аккаунта после последнего sync или выплаты
}

pub struct Milestone {
    title: String,
    weight_bps: u16,
    deadline_ms: u64,
    status: MilestoneStatus,
    proof_uri: Option<String>,
    proof_hash: Option<String>,    // sha256, 64 hex-символа в нижнем регистре
    submitted_at_ms: Option<u64>,
    challenger: Option<AccountId>,
    challenge_reason: Option<String>,
}

pub enum MilestoneStatus { Pending, Submitted, Challenged, Accepted, Rejected, Failed }
```

## 5. Учёт денег

### 5.1 Синхронизация

- В конце `new`: `last_balance = env::account_balance()`. Стартовый депозит — не доход.
- `sync()`:
  - `b = env::account_balance()`;
  - если `b > last_balance`: `income = b - last_balance`, `total_received += income`,
    событие `fees_synced`;
  - в любом случае `last_balance = b`.
- Баланс уменьшается только нашими выплатами, а `claim` сам уменьшает `last_balance`.
  Поэтому всё, что выросло сверх `last_balance`, — новый доход.
- Контракту достаётся 30% газа, сожжённого в его методах. Это копейки, они тоже попадут
  в доход. В тестах суммы сравниваются с допуском.

### 5.2 Формулы

Все деления — целочисленные, с округлением вниз. Умножения — через `checked_mul`.

```
R      = total_received
I      = R * immediate_bps / 10000          // сразу
L      = R - I                              // заложено под этапы
W_acc  = сумма weight_bps этапов Accepted
W_lost = сумма weight_bps этапов Rejected и Failed
U      = L * W_acc / 10000                  // разлочено
F      = L * W_lost / 10000                 // сгорело
P      = L - U - F                          // ещё ждёт
claimable = I + U - claimed
```

Формулы считаются от всего дохода `R`, поэтому работают и для будущих фисов: после
провала этапа его доля будущего дохода тоже сгорает пропорционально весу.

Пример. Пришло 100 NEAR, `immediate_bps = 5000`, этапы 40/30/30.
Сразу 50, заложено 50. Этап 1 принят: разлочено 20. Этап 2 отклонён: сгорело 15.
Этап 3 ждёт: 15. Креатор раньше забрал 30 → доступно 50 + 20 − 30 = 40 NEAR.

### 5.3 Выплата

`claim()` — только `creator`:

1. Вызвать `sync()`.
2. `amount = claimable`.
3. Ограничить: `amount ≤ account_balance − storage_usage × storage_byte_cost`,
   чтобы не тронуть деньги, которые держат хранение.
4. Если `amount == 0` → ошибка `"Nothing to claim"`.
5. `claimed += amount`, `last_balance -= amount`.
6. `Promise::new(creator).transfer(amount)`, затем колбэк `on_claim_transfer(amount)`.
7. Колбэк (`#[private]`): если перевод не прошёл — `claimed -= amount`,
   `last_balance += amount`, событие `claim_failed`; иначе событие `claimed`.

## 6. Этапы и статусы

| Переход | Метод | Кто | Условие |
| --- | --- | --- | --- |
| Pending → Submitted | `submit_proof` | только `creator` | `now < deadline_ms` |
| Submitted → Challenged | `challenge` | любой, кроме `creator` | `now < submitted_at_ms + challenge_window_ms` |
| Submitted → Accepted | `finalize` | любой | `now ≥ submitted_at_ms + challenge_window_ms` |
| Challenged → Accepted / Rejected | `resolve` | только `verifier` | — |
| Pending → Failed | `expire` | любой | `now ≥ deadline_ms` |

- Accepted, Rejected и Failed — конечные статусы.
- Пруф, поданный до дедлайна, проходит, даже если окно спора закончится после дедлайна.
- Оспорить пруф можно только один раз: после `challenge` статус уже Challenged.
- Время — `env::block_timestamp_ms()`.

## 7. Методы

```rust
#[init]
pub fn new(creator: AccountId, verifier: AccountId, goal_title: String, goal_uri: String,
           immediate_bps: u16, challenge_window_ms: u64,
           milestones: Vec<MilestoneInput>) -> Self

pub fn set_token(&mut self, token: AccountId)                  // creator, один раз
#[payable] pub fn deposit(&mut self)                            // любой, затем sync
pub fn sync(&mut self)                                          // любой
pub fn submit_proof(&mut self, index: u8, proof_uri: String, proof_hash: String) // creator
pub fn challenge(&mut self, index: u8, reason: String)          // любой, кроме creator
pub fn finalize(&mut self, index: u8)                           // любой
pub fn resolve(&mut self, index: u8, accept: bool)              // verifier
pub fn expire(&mut self, index: u8)                             // любой
pub fn claim(&mut self) -> Promise                              // creator
#[private] pub fn on_claim_transfer(&mut self, amount: NearToken)

// view
pub fn get_goal(&self) -> GoalView          // параметры, этапы, статусы
pub fn get_balances(&self) -> BalancesView  // total_received, immediate, locked, unlocked,
                                            // forfeited, pending, claimed, claimable,
                                            // unsynced = account_balance − last_balance
```

- Все суммы в JSON — строки в yoctoNEAR (`NearToken` так и сериализуется).
- Методы без `#[payable]` отклоняют приложенный депозит — это поведение near-sdk по умолчанию.
- Метода, который меняет creator, verifier, веса, `immediate_bps` или окно, нет и не будет.

Лимиты строк: `goal_title` и `title` этапа ≤ 100, `goal_uri` и `proof_uri` ≤ 256,
`reason` ≤ 280, `proof_hash` — ровно 64 hex-символа.

Сообщения об ошибках — короткие и понятные: `"Only creator"`, `"Only verifier"`,
`"Wrong status"`, `"Deadline passed"`, `"Window closed"`, `"Window still open"`,
`"Nothing to claim"`, `"Weights must sum to 10000"`, `"No such milestone"`,
`"Invalid proof hash"`, `"Creator cannot challenge"`, `"Reason too long"`,
`"Deadline not passed"`.

## 8. События (NEP-297)

Формат: `EVENT_JSON:{"standard":"proofee","version":"1.0.0","event":"<name>","data":[{...}]}`

| Событие | Данные |
| --- | --- |
| `goal_created` | creator, verifier, milestones (количество) |
| `fees_synced` | income, total_received |
| `proof_submitted` | index, proof_uri, proof_hash |
| `proof_challenged` | index, challenger, reason |
| `milestone_resolved` | index, status, by |
| `claimed` | amount, total_claimed |
| `claim_failed` | amount |

## 9. Безопасность

- Сначала меняем состояние, потом отправляем деньги; при неудачном переводе колбэк
  откатывает счётчики.
- Округление вниз: контракт не может заплатить больше, чем получил.
- В каждом методе проверяем, кто вызвал (`env::predecessor_account_id()`), статус этапа
  и время.
- Деньги под хранение не выплачиваем (п. 5.3).
- После деплоя удаляем full access ключ и проверяем, что ключей не осталось.

## 10. Тесты

Юнит-тесты (`near-sdk` с `VMContextBuilder`):

1. `new` отклоняет веса, сумма которых не 10000.
2. `new` отклоняет 0 или больше 5 этапов, а также `verifier == creator`.
3. `sync` считает входящий перевод доходом, стартовый депозит — нет.
4. Деление на «сразу» и «заложено» совпадает с формулой (пример из п. 5.2).
5. `submit_proof`: только creator, только из Pending, только до дедлайна, хеш проверяется.
6. `challenge`: только пока окно открыто, не от creator.
7. `finalize` после окна → Accepted, разлочивается нужная сумма.
8. `resolve`: только verifier; отказ сжигает долю этапа, в том числе из будущих фисов.
9. `expire` после дедлайна → Failed.
10. `claim`: платит ровно доступное, повторный claim → `"Nothing to claim"`, чужой аккаунт
    забрать не может, колбэк откатывает счётчики при неудаче.

Интеграционный тест (`near-workspaces`, песочница):

11. Деплой → перевод 10 NEAR → `sync` → пруф → ожидание окна → `finalize` → `claim` →
    проверка балансов креатора и контракта.

## 11. Демо-сценарий в testnet

Синтаксис near-cli-rs сверять с `near --help`.

1. Задеплоить цель: окно 120000 мс, три этапа 40/30/30, дедлайны через несколько дней.
2. Имитировать фисы: перевести 1 NEAR на контракт
   (`near tokens <sender>.testnet send-near <goal>.testnet '1 NEAR' network-config testnet sign-with-keychain send`).
3. `sync` → `get_balances`: claimable 0,5 NEAR.
4. `submit_proof` по этапу 0 → подождать 2 минуты → `finalize` → claimable 0,7 NEAR.
5. `submit_proof` по этапу 1 → `challenge` с третьего аккаунта → `resolve(1, false)` →
   forfeited 0,15 NEAR.
6. `claim` → NEAR пришли креатору.
7. Перевести ещё 1 NEAR → `sync` → видно, что доля отклонённого этапа сгорает и из
   новых фисов.

## 12. Связь с Nearly (для mainnet, после хакатона)

- `fee_to` = адрес контракта цели.
- NEAR-часть фисов приходит обычными переводами от `nearlytrade.near` — `sync` их видит.
- Часть фисов Nearly учитывает в самом токене (`creator_token_fees`), выплата через
  `push_creator_token_fees` не проверена. Контракт MVP токены не учитывает — до mainnet
  нужен `sync_ft` и выплата токенов.
- Не проверено, может ли креатор сменить `fee_to` после запуска
  (`set_launch_recipients`, `set_launch_hook`). Если может — он обходит контракт.
  Проверить до mainnet.
- Buy/Sell tax в демо-запусках — 0.
