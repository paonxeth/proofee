# ProoFee Launcher + LP Locker — Testnet E2E

Дата: 12 ноября 2026. Только NEAR Testnet. P0 и on-chain проверки P1 выполнены.
Это MVP без независимого аудита, не готовый Mainnet deployment.

## Реализовано

Последовательный Node.js Launcher на основе проверенного PoC, отдельный Rust Locker
на каждый токен, NEP-141 без дополнительной эмиссии, pool token/wrap.testnet fee=10000,
HotZap всего initial supply, Buy/Sell, LP fee collection, конвертация token fee в wNEAR,
точный unwrap, native NEAR в новый Goal, sync и Creator Claim.
Это CLI/library orchestration, не многоарендный on-chain Factory и не размещённый web API.
Существующий Goal и Treasury не изменены. Goal WASM собран из исходника основного
репозитория без изменений и развёрнут на новом изолированном аккаунте.

## Аккаунты и позиция

| Назначение | ID |
|---|---|
| Isolated root / creator | `pf-launch-mwdyyhj0.testnet` |
| Token | `token-e2e1.pf-launch-mwdyyhj0.testnet` |
| Locker / LP owner | `lock-e2e1.pf-launch-mwdyyhj0.testnet` |
| Новый Goal | `goal-e2e1.pf-launch-mwdyyhj0.testnet` |
| Permissionless caller | `caller.pf-launch-mwdyyhj0.testnet` |
| DCL | `dclv2.ref-dev.testnet` |
| wNEAR | `wrap.testnet` |

Pool: `token-e2e1.pf-launch-mwdyyhj0.testnet|wrap.testnet|10000`.

LPT: `token-e2e1.pf-launch-mwdyyhj0.testnet|wrap.testnet|10000#2583`.

Token decimals=18, supply=1000000000000000000000000000 raw = 1 000 000 000 токенов.
Init=-69400, range=[-69200,-59400], без начального wNEAR. LP создаётся сразу от Locker.

В DCL отправлен весь supply; непосредственно в LP попало 999999999.99999999999987968
токена. Остаток 120320 raw = 0.00000000000012032
токена — DCL integer-rounding principal dust, постоянно остающийся на внутреннем
балансе Locker и исключённый из сборов. Строгое «каждая raw unit находится в LP»
не выполнено из-за округления DCL; этот остаток не называется LP-ликвидностью.

## Проверенный lock

У Token, Locker и Goal нулевые списки access keys. Code hash каждого совпал с
поставленным WASM; детали в evidence/onchain-final.json. У Locker нет метода upgrade,
положительного remove_liquidity, произвольного вызова, NFT transfer/approval,
изменения Goal или вывода всего native-баланса.

DCL owner_id позиции = Locker. Principal liquidity amount до и после всех сделок и
трёх collect-циклов: `4057331146356777986151`. Creator и deployment root не смогли вывести
principal. Попытка deployment root переустановить тот же WASM на keyless Locker
отклонена NEAR. Зависимость от администрирования и обновлений внешнего DCL сохраняется.

## Комиссии и Goal

После первоначальных Buy/Sell:

| Актив | Общая swap fee | Protocol fee DCL | Комиссия нашей LP |
|---|---:|---:|---:|
| Token, raw | 482234820568394840715492 | 96446964113678968143098 | 385787856454715872572393 |
| wNEAR, raw | 1000000000000000000000 | 200000000000000000000 | 799999999999999999999 |

Protocol доля = 20% swap fee, LP около 80%; ещё по 1 raw unit приходится на округление.
В этом deployment pool.total_fee_*_charged совпадает с protocol_fee события swap;
это не вся комиссия пула и не доход ProoFee. Доли подтверждены событиями, а не именем поля.

Token-комиссия LP: 385787.856454715872572393 токена. wNEAR-комиссия LP:
0.000799999999999999999999 wNEAR. Swap event первой fee-конвертации сообщает
amount_in=385787856454715872572383, на 10 raw units
меньше переданной FT-суммы; эта DCL-особенность округления сохранена в evidence,
не объявляется дополнительным доходом или подтверждённым refund.
Финальная wallet token balance Locker = 0; DCL internal assets = исходный principal dust.

Первый цикл доставил в Goal ровно **0.001191987982122365775599 NEAR**:
фактический wNEAR-выход fee swap плюс отдельная wNEAR LP fee. После sync Goal начислил
20% immediate, 80% milestones; первый Creator Claim = **0.000238397596424473155119 NEAR**.

После повторного collect и permissionless collect от другого аккаунта, финальных sync/claim:

| Показатель | NEAR |
|---|---:|
| Доставлено Locker / учтено Goal | 0.0011951483583858763164 |
| Immediate 20%, выплачено creator | 0.00023902967167717526328 |
| Locked milestones 80% | 0.00095611868670870105312 |
| Unsynced / claimable | 0 / 0 |

Повторные collect получают новые комиссии от предыдущих fee-conversion trades.
Это не повторная выплата исходного дохода: сверены новые unclaimed_fee и отдельные
wNEAR settlement amounts. В LP остаётся небольшая новая token fee от последней
конвертации: 197523382504814527 raw, ещё не собранная и не записанная как native income.

## Проверки и ошибки

14 unit-тестов, 25 проверок evidence прошли. Финальность: 62/62 включённых транзакций FINAL,
54 успешных, 8 неудачных. Неудачные: исходный недостаточный storage deposit нового Goal
(атомарный откат, затем успешный запуск с 2.1 NEAR) и 7 отрицательных тестовых транзакций,
включая повтор проверки запрета parent upgrade. На существующий goal-4 не отправлялось операций.

Негативные проверки: отсутствие principal-withdraw и goal-setter в Locker;
private callback от постороннего; DCL remove_liquidity от не-владельца;
parent deploy без ключей; чужой claim. Посторонний caller выполнил collect с лишними
receiver_id/goal_id: они не изменили immutable Goal, caller получил только расход газа.

Отдельно от включённых транзакций:
- Одна попытка отклонена до включения: недостаточно баланса caller для резерва 300 TGas.
- Одна подписанная попытка получила RPC 429 на deprecated rpc.testnet.near.org и не была
  включена. Не-включение подтверждено: тот же signer/public key/nonce позже потреблён
  другой FINAL-транзакцией. Доказательство — evidence/superseded-transactions.json.

Переключение RPC и продолжение с сохранённого on-chain этапа не выпускали supply повторно.
В клиент добавлены ожидание FINAL и проверка chain_id=testnet перед подписью.

Суммарно tokens_burnt всех 62 транзакций, включая faucet и negative tests:
**0.0891601768188389 NEAR**. Это газ, не deployment/storage funding,
и не gas limit 300 TGas. Пооперационная детализация — evidence/transactions.csv.

## Основные транзакции

| Операция | Testnet transaction |
|---|---|
| create-token | [41AvUu6eWBYEhv1FiEajii3GRrjq2WqFo7LoZjfaxqGr](https://testnet.nearblocks.io/txns/41AvUu6eWBYEhv1FiEajii3GRrjq2WqFo7LoZjfaxqGr) |
| create-locker | [9udHWdV2DxjUW4eS2Ljp7Ffz7We1zSNSWvaB9URQW2BX](https://testnet.nearblocks.io/txns/9udHWdV2DxjUW4eS2Ljp7Ffz7We1zSNSWvaB9URQW2BX) |
| e2e-buy | [6GWzhE7XUmyRfCYNPduvzBaw1iCWGMDaYi8pWMTRtHeT](https://testnet.nearblocks.io/txns/6GWzhE7XUmyRfCYNPduvzBaw1iCWGMDaYi8pWMTRtHeT) |
| e2e-sell | [4ZHXdbFbLzzLa7XRXRAypgYiFvYqGG8Q69hUR5acDUhg](https://testnet.nearblocks.io/txns/4ZHXdbFbLzzLa7XRXRAypgYiFvYqGG8Q69hUR5acDUhg) |
| e2e-collect | [Ba8atbQh3LR7Fu8e2Z21wFxWQHHqLhZjVxHxJxPGBezo](https://testnet.nearblocks.io/txns/Ba8atbQh3LR7Fu8e2Z21wFxWQHHqLhZjVxHxJxPGBezo) |
| e2e-goal-sync | [5yw4UUGkiphvjdETJcgmmfFqz7ZHioqAQcBKfDpg9P4U](https://testnet.nearblocks.io/txns/5yw4UUGkiphvjdETJcgmmfFqz7ZHioqAQcBKfDpg9P4U) |
| e2e-creator-claim | [87itFbHTFWLaoYNQpUguTn7TZ5qoy57YnsykDiz2g7D3](https://testnet.nearblocks.io/txns/87itFbHTFWLaoYNQpUguTn7TZ5qoy57YnsykDiz2g7D3) |
| final-goal-sync | [DEGne4vAaLupoBFJbnh4sv7qBUnzKRtxrSFznQ6w4xcN](https://testnet.nearblocks.io/txns/DEGne4vAaLupoBFJbnh4sv7qBUnzKRtxrSFznQ6w4xcN) |
| final-creator-claim | [DRobdxz39jxFSyPsMM6Uj3XdLtXB1AxnnfkqLt8UaLH3](https://testnet.nearblocks.io/txns/DRobdxz39jxFSyPsMM6Uj3XdLtXB1AxnnfkqLt8UaLH3) |
| negative-parent-upgrade | [1aTfJ1tzGTEVQB3JVkAJgu92JJ7MWzUiZEL2PCKvP2w](https://testnet.nearblocks.io/txns/1aTfJ1tzGTEVQB3JVkAJgu92JJ7MWzUiZEL2PCKvP2w) |
| negative-stranger-claim | [2mALkQhjZXgeN1c8NH5kqaxnVjcKBk1SibSkL5yfjkPa](https://testnet.nearblocks.io/txns/2mALkQhjZXgeN1c8NH5kqaxnVjcKBk1SibSkL5yfjkPa) |

Полный перечень — evidence/final-transactions.json и transactions.csv. Все raw RPC,
child receipt outcomes, balances и подписи транзакций доступны в release archive
в full-evidence/. Приватных signing keys там нет.

## Ограничения и передача

- Locker immutable/keyless. При неоднозначной state-changing receipt или потерянном
  callback он останавливается без admin-reset; восстановление всех таких случаев не
  гарантировано. Известные read/pay failures и полностью возвращённый seed/swap допускают retry.
- Fee swap имеет 1% floor от текущей DCL quote, без oracle/MEV-защиты.
- Целый supply отправлен в DCL, но математическое округление оставляет указанный locked dust.
- Launcher — CLI/library; frontend/backend service не развёрнут. Handoff описывает интеграцию.
- Второй PoC-архив недоступен. Использованы первый архив и действующий Goal ABI из репозитория.
- Ветка локальная `feat/testnet-launcher-locker`. GitHub подключение предоставляет pull=true,
  push=false; удалённая ветка/PR не созданы. Патч добавляет только launcher/.
- Сгенерированные Testnet signer-файлы находятся вне репозитория и release; их нужно сохранить
  в секретном хранилище для будущего Creator Claim. Значения ключей не публикуются.

Запуск, ABI и frontend инструкции: README.md, INTEGRATION_HANDOFF.md, abi/.
Происхождение и хеши WASM: SOURCE_PROVENANCE.json. Treasury и рабочие team accounts не затронуты.
