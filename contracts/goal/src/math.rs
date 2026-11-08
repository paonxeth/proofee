// Формулы учёта денег (SPEC 5.2). Чистые функции: на входе и выходе — yoctoNEAR в u128,
// никакого состояния контракта. Деление округляет вниз, остальное — checked-арифметика.
use near_sdk::env;

pub const BPS_DENOMINATOR: u16 = 10_000;

#[derive(Debug, PartialEq)]
pub struct Breakdown {
    pub immediate: u128,
    pub locked: u128,
    pub unlocked: u128,
    pub forfeited: u128,
    pub pending: u128,
    pub claimable: u128,
}

pub fn overflow() -> ! {
    env::panic_str("Math overflow")
}

/// `amount * bps / 10000`, с округлением вниз.
pub fn bps_share(amount: u128, bps: u32) -> u128 {
    amount
        .checked_mul(u128::from(bps))
        .unwrap_or_else(|| overflow())
        / u128::from(BPS_DENOMINATOR)
}

/// Раскладка дохода `total_received` по SPEC 5.2.
/// `w_acc` — сумма весов Accepted, `w_lost` — Rejected и Failed.
pub fn compute(
    total_received: u128,
    immediate_bps: u16,
    w_acc: u32,
    w_lost: u32,
    claimed: u128,
) -> Breakdown {
    let immediate = bps_share(total_received, u32::from(immediate_bps));
    let locked = total_received
        .checked_sub(immediate)
        .unwrap_or_else(|| overflow());
    let unlocked = bps_share(locked, w_acc);
    let forfeited = bps_share(locked, w_lost);
    let pending = locked
        .checked_sub(unlocked)
        .and_then(|x| x.checked_sub(forfeited))
        .unwrap_or_else(|| overflow());
    let claimable = immediate
        .checked_add(unlocked)
        .and_then(|x| x.checked_sub(claimed))
        .unwrap_or_else(|| overflow());

    Breakdown {
        immediate,
        locked,
        unlocked,
        forfeited,
        pending,
        claimable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEAR: u128 = 1_000_000_000_000_000_000_000_000;

    // SPEC 10, тест 4: пример из 5.2.
    #[test]
    fn spec_example_100_near() {
        // Этапы 40/30/30: первый принят, второй отклонён, третий ждёт. Забрано 30.
        let b = compute(100 * NEAR, 5000, 4000, 3000, 30 * NEAR);
        assert_eq!(
            b,
            Breakdown {
                immediate: 50 * NEAR,
                locked: 50 * NEAR,
                unlocked: 20 * NEAR,
                forfeited: 15 * NEAR,
                pending: 15 * NEAR,
                claimable: 40 * NEAR,
            }
        );
    }

    #[test]
    fn rounds_down_without_losing_yocto() {
        let b = compute(1001, 3333, 3333, 3334, 0);
        assert_eq!(b.immediate, 333); // 333.6
        assert_eq!(b.locked, 668);
        assert_eq!(b.unlocked, 222); // 222.6
        assert_eq!(b.forfeited, 222); // 222.7
        assert_eq!(b.pending, 224);
        assert_eq!(b.immediate + b.unlocked + b.forfeited + b.pending, 1001);
        assert_eq!(b.claimable, 555);
    }

    #[test]
    #[should_panic(expected = "Math overflow")]
    fn claimed_above_available_panics() {
        compute(100, 5000, 0, 0, 51);
    }
}
