use super::*;

#[test]
fn bma_independent_quantlib_prices_and_coupon() {
    let m = Market::new();
    let helpers: Vec<_> = YEARS
        .iter()
        .zip(FRACTIONS)
        .map(|(&y, f)| m.helper(y, &shared(SimpleQuote::new(f))) as Shared<dyn RateHelper>)
        .collect();
    let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        m.today,
        helpers.clone(),
        Actual360::new(),
        LogLinear,
    )
    .unwrap();
    let bma = shared(
        m.bma
            .clone_with(Handle::new(curve.clone() as Shared<dyn YieldTermStructure>)),
    );
    assert_eq!(
        include_str!("../../../tests/fixtures/bma/swaps.csv")
            .lines()
            .count(),
        YEARS.len() + 1
    );
    for (i, line) in include_str!("../../../tests/fixtures/bma/swaps.csv")
        .lines()
        .skip(1)
        .enumerate()
    {
        let row: Vec<f64> = line.split(',').map(|s| s.parse().unwrap()).collect();
        assert_eq!(row.len(), 11);
        let mut swap = m.swap(YEARS[i], bma.clone());
        assert_eq!(helpers[i].pillar_date().serial_number(), row[2] as i32);
        let actual = [
            curve
                .discount_date(helpers[i].pillar_date(), false)
                .unwrap(),
            swap.fair_libor_fraction().unwrap(),
            swap.fair_libor_spread().unwrap(),
            swap.npv().unwrap(),
            swap.swap_mut().leg_npv(0).unwrap(),
            swap.swap_mut().leg_npv(1).unwrap(),
            swap.swap_mut().leg_bps(0).unwrap(),
            swap.swap_mut().leg_bps(1).unwrap(),
        ];
        for (value, expected) in actual.iter().zip(&row[3..]) {
            assert!(
                (value - expected).abs() < 1e-9,
                "tenor {}: {value} vs {expected}",
                YEARS[i]
            );
        }
    }
    let end = Date::new(27, Month::January, 2026);
    let coupon = AverageBMACoupon::new(
        end,
        100.,
        m.settlement,
        end,
        bma,
        1.2,
        0.001,
        None,
        None,
        Actual360::new(),
    )
    .unwrap();
    assert!((coupon.rate().unwrap() - 0.034118766395668354).abs() < 1e-12);
    assert!((coupon.amount().unwrap() - 0.8719240301115245).abs() < 1e-10);
}

fn bma_consistency<T, I, B>(tolerance: f64)
where
    T: crate::termstructures::bootstraptraits::YieldBootstrapTraits + 'static,
    I: crate::math::interpolations::Interpolator + Default + 'static,
    B: crate::termstructures::iterativebootstrap::Bootstrap<PiecewiseYieldCurve<T, I, B>>
        + Default
        + 'static,
{
    let m = Market::new();
    let helpers = YEARS
        .iter()
        .zip(FRACTIONS)
        .map(|(&y, f)| m.helper(y, &shared(SimpleQuote::new(f))) as Shared<dyn RateHelper>)
        .collect();
    let curve =
        PiecewiseYieldCurve::<T, I, B>::new(m.today, helpers, Actual360::new(), I::default())
            .unwrap();
    let bma = shared(
        m.bma
            .clone_with(Handle::new(curve as Shared<dyn YieldTermStructure>)),
    );
    for (&year, fraction) in YEARS.iter().zip(FRACTIONS) {
        let actual = m.swap(year, bma.clone()).fair_libor_fraction().unwrap();
        assert!(
            (actual - fraction).abs() <= tolerance,
            "{} {year}: {actual} vs {fraction}",
            std::any::type_name::<I>()
        );
    }
}

#[derive(Default)]
struct MonotonicSpline;
impl crate::math::interpolations::Interpolator for MonotonicSpline {
    type Output = crate::math::interpolations::cubic::CubicInterpolation;
    const GLOBAL: bool = true;
    fn interpolate(&self, x: &[f64], y: &[f64]) -> QlResult<Self::Output> {
        crate::math::interpolations::cubic::MonotonicCubicNaturalSpline::new(x.to_vec(), y.to_vec())
    }
}

#[test]
fn bma_upstream_curve_and_bootstrap_variants() {
    use crate::math::interpolations::{
        convexmonotone::ConvexMonotone, flat::BackwardFlat, linear::Linear,
    };
    use crate::termstructures::{
        bootstraptraits::{ForwardRate, ZeroYield},
        iterativebootstrap::IterativeBootstrap,
        localbootstrap::LocalBootstrap,
    };
    bma_consistency::<Discount, Linear, IterativeBootstrap>(1e-9);
    bma_consistency::<ZeroYield, Linear, IterativeBootstrap>(1e-9);
    bma_consistency::<ZeroYield, MonotonicSpline, IterativeBootstrap>(1e-9);
    bma_consistency::<ForwardRate, Linear, IterativeBootstrap>(1e-9);
    bma_consistency::<ForwardRate, BackwardFlat, IterativeBootstrap>(1e-9);
    bma_consistency::<ForwardRate, ConvexMonotone, IterativeBootstrap>(1e-9);
    bma_consistency::<ForwardRate, ConvexMonotone, LocalBootstrap>(1e-6);
}

#[test]
fn bma_preceding_payment_can_precede_unadjusted_accrual_end() {
    use crate::cashflows::AverageBMALeg;
    use crate::time::schedule::Schedule;
    let m = Market::new();
    let bma = shared(m.bma.clone_with(m.ibor.forwarding_term_structure().clone()));
    let start = Date::new(1, Month::January, 2026);
    let end = Date::new(25, Month::January, 2026);
    let schedule = Schedule::with_metadata(
        vec![start, end],
        m.bma.fixing_calendar(),
        Bdc::Unadjusted,
        None,
        None,
        None,
        None,
        vec![],
    );
    let leg = AverageBMALeg::new(schedule, bma)
        .with_notional(100.)
        .with_payment_adjustment(Bdc::Preceding)
        .build()
        .unwrap();
    assert_eq!(leg[0].date(), Date::new(23, Month::January, 2026));
    assert!(leg[0].amount().unwrap().is_finite());
}

#[test]
fn bma_nonfinite_coupon_results_fail_and_history_recovers() {
    let m = Market::new();
    let start = Date::new(27, Month::December, 2024);
    let end = Date::new(3, Month::January, 2025);
    let fixing = Date::new(26, Month::December, 2024);
    let c = AverageBMACoupon::new(
        end,
        100.,
        start,
        end,
        m.bma.clone(),
        1.,
        0.,
        None,
        None,
        Actual360::new(),
    )
    .unwrap();
    m.bma.add_fixing(fixing, f64::MAX / 2.).unwrap();
    assert!(c.rate().is_err());
    m.bma.clear_fixings();
    m.bma.add_fixing(fixing, 0.03).unwrap();
    assert!((c.rate().unwrap() - 0.03).abs() < 1e-15);
    assert!(
        AverageBMACoupon::new(
            Date::null(),
            100.,
            start,
            end,
            m.bma,
            1.,
            0.,
            None,
            None,
            Actual360::new()
        )
        .is_err()
    );
}
