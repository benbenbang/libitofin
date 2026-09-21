use super::*;
use crate::exercise::{BermudanExercise, EuropeanExercise, Exercise};
use crate::handle::Handle;
use crate::indexes::ibor::Euribor;
use crate::instruments::{SettlementMethod, SettlementType, SwapType, Swaption};
use crate::interestrate::Compounding;
use crate::shared::shared;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::target::Target;
use crate::time::date::Month;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounters::actual360::Actual360;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

fn date(day: i32) -> Date {
    Date::new(day, Month::January, 2027)
}

fn fixture() -> (SwaptionArguments, Shared<Settings<Date>>) {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(2, Month::January, 2026));
    let curve = Handle::new(shared(FlatForward::with_rate(
        Date::new(2, Month::January, 2026),
        0.03,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    let index = shared(Euribor::six_months(curve, Shared::clone(&settings)));
    let fixed = Schedule::with_metadata(
        vec![date(8), date(29)],
        Target::new(),
        BusinessDayConvention::ModifiedFollowing,
        Some(BusinessDayConvention::Unadjusted),
        Some(Period::new(1, TimeUnit::Months)),
        Some(DateGeneration::Backward),
        Some(true),
        vec![false],
    );
    let floating = Schedule::from_dates(vec![date(22), date(29)]);
    let swap = VanillaSwap::new(
        SwapType::Payer,
        100.0,
        fixed,
        0.04,
        Actual360::new(),
        floating,
        index,
        0.0,
        Actual360::new(),
        None,
        Shared::clone(&settings),
    )
    .unwrap();
    let swaption = Swaption::new(
        shared_mut(swap.into_fixed_vs_floating()),
        shared(EuropeanExercise::new(date(15))) as Shared<dyn Exercise>,
        SettlementType::Physical,
        SettlementMethod::PhysicalOTC,
        Shared::clone(&settings),
    );
    let mut args = SwaptionArguments::default();
    swaption.setup_arguments(&mut args).unwrap();
    (args, settings)
}

#[test]
fn seven_day_boundary_is_inclusive_and_terminal_date_is_untouched() {
    for offset in -8..=8 {
        let original = date(15) + offset;
        let mut dates = vec![original, date(29)];
        let adjustments = snap_dates(&mut dates, &[date(15)]).unwrap();
        assert_eq!(
            dates[0],
            if offset.abs() <= 7 {
                date(15)
            } else {
                original
            }
        );
        assert_eq!(dates[1], date(29));
        assert_eq!(
            adjustments[0],
            if (-7..0).contains(&offset) {
                CouponAdjustment::Post
            } else {
                CouponAdjustment::Pre
            }
        );
    }
    let mut dates = vec![date(1), date(15)];
    snap_dates(&mut dates, &[date(22)]).unwrap();
    assert_eq!(dates, [date(1), date(15)]);
}

#[test]
fn rebuilds_coupon_amounts_and_accruals_without_mutating_original_swap() {
    let (args, settings) = fixture();
    let original = args.swap.as_ref().unwrap();
    let fixed_dates = original.borrow().fixed_schedule().dates().to_vec();
    let floating_dates = original.borrow().floating_schedule().dates().to_vec();
    let prepared = prepare_swaption_with_snapped_dates(&args).unwrap();
    assert_eq!(prepared.arguments.fixed_reset_dates, [date(15)]);
    assert_eq!(prepared.arguments.floating_reset_dates, [date(15)]);
    assert_eq!(prepared.arguments.fixed_pay_dates, [date(29)]);
    assert_eq!(prepared.arguments.floating_pay_dates, [date(29)]);
    assert_eq!(prepared.fixed_adjustments, [CouponAdjustment::Post]);
    assert_eq!(prepared.floating_adjustments, [CouponAdjustment::Pre]);
    assert!((prepared.arguments.fixed_coupons[0] - 100.0 * 0.04 * 14.0 / 360.0).abs() < 1e-12);
    assert!((prepared.arguments.floating_accrual_times[0] - 14.0 / 360.0).abs() < 1e-12);
    assert!((args.swap_arguments.fixed_coupons[0] - 100.0 * 0.04 * 21.0 / 360.0).abs() < 1e-12);
    assert!((args.swap_arguments.floating_accrual_times[0] - 7.0 / 360.0).abs() < 1e-12);
    assert_eq!(
        settings.evaluation_date().unwrap(),
        Date::new(2, Month::January, 2026)
    );
    assert_eq!(original.borrow().fixed_schedule().dates(), fixed_dates);
    assert_eq!(
        original.borrow().floating_schedule().dates(),
        floating_dates
    );
}

#[test]
fn malformed_sources_and_collapsed_schedules_are_errors() {
    let (mut args, _) = fixture();
    args.swap = None;
    assert!(prepare_swaption_with_snapped_dates(&args).is_err());
    let (mut args, _) = fixture();
    args.exercise = Some(shared(
        BermudanExercise::new(vec![Date::null()], false).unwrap(),
    ));
    assert!(prepare_swaption_with_snapped_dates(&args).is_err());
    assert!(snap_dates(&mut [], &[date(15)]).is_err());
    assert!(snap_dates(&mut [date(8), date(16), date(29)], &[date(15)]).is_err());
}

#[test]
fn overnight_underlying_is_not_reinterpreted_as_a_vanilla_swap() {
    use crate::cashflows::RateAveraging;
    use crate::indexes::ibor::Eonia;
    use crate::instruments::OvernightIndexedSwap;
    use crate::time::businessdayconvention::BusinessDayConvention;

    let (mut args, settings) = fixture();
    let original = args.swap.as_ref().unwrap().borrow();
    let index = shared(Eonia::new(
        original.ibor_index().forwarding_term_structure().clone(),
        Shared::clone(&settings),
    ));
    let swap = OvernightIndexedSwap::with_nominal(
        SwapType::Payer,
        100.0,
        original.fixed_schedule().clone(),
        0.04,
        Actual360::new(),
        original.fixed_schedule().clone(),
        index,
        0.0,
        0,
        BusinessDayConvention::Unadjusted,
        None,
        RateAveraging::Compound,
        settings,
    )
    .unwrap();
    drop(original);
    args.swap = Some(shared_mut(swap.into_fixed_vs_floating()));
    assert!(matches!(
        prepare_swaption_with_snapped_dates(&args),
        Err(e) if e.message() == "date snapping requires a vanilla Ibor swap"
    ));
}
