use super::*;
use crate::calendar_api::NativeCalendar;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::ZeroCouponBond;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::NullCalendar;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use libitofin::time::frequency::Frequency;
use std::ptr::null_mut;

fn fixture(lag: u32) -> (Context, u64, u64, u64, u64, u64) {
    let mut c = Context::new();
    let settings = shared(Settings::<Date>::new());
    let calendar = NativeCalendar {
        inner: NullCalendar::new(),
        horizon: Some(2014),
        first_year: 1901,
    };
    let start = crate::time_api::date(46_199).unwrap();
    let end = crate::time_api::date(46_929).unwrap();
    let bond = ZeroCouponBond::new(
        lag,
        calendar.inner.clone(),
        375.0,
        end,
        BusinessDayConvention::Unadjusted,
        102.0,
        Some(start),
        settings.clone(),
    )
    .unwrap();
    let native = NativeBond::new(bond.into_bond(), calendar, None).unwrap();
    let leg = native.bond.borrow().cashflows().clone();
    let bond = c.insert(native).unwrap();
    let leg = c.insert(leg).unwrap();
    let settings = c.insert(settings).unwrap();
    let dc: DayCounter = Actual360::new();
    let dc = c.insert(dc).unwrap();
    let curve = FlatForward::with_rate(
        start,
        0.032,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    );
    let curve: Handle<dyn YieldTermStructure> =
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>);
    let curve = c.insert(curve).unwrap();
    (c, bond, leg, settings, dc, curve)
}
fn defaults() -> ItofinAnalyticsSolverConfig {
    ItofinAnalyticsSolverConfig {
        flags: 0,
        accuracy: 0.0,
        max_evaluations: 0,
        guess: 0.0,
    }
}
#[test]
fn analytics_checked_conventions_and_solver_presence() {
    let variants = [
        Compounding::Simple,
        Compounding::Compounded,
        Compounding::Continuous,
        Compounding::SimpleThenCompounded,
        Compounding::CompoundedThenSimple,
    ];
    for (i, expected) in variants.into_iter().enumerate() {
        assert_eq!(compounding(i as i32).unwrap(), expected);
    }
    for value in [-1, 5, i32::MAX] {
        assert!(compounding(value).is_err());
    }
    for (i, expected) in [
        Frequency::Annual,
        Frequency::Semiannual,
        Frequency::Quarterly,
        Frequency::Monthly,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(frequency(i as i32).unwrap(), expected);
    }
    let absent = solver(defaults()).unwrap();
    assert!(absent.accuracy.is_none() && absent.evaluations.is_none() && absent.guess.is_none());
    let explicit = solver(ItofinAnalyticsSolverConfig {
        flags: 4,
        ..defaults()
    })
    .unwrap();
    assert_eq!(explicit.guess, Some(0.0));
    for a in [
        ItofinAnalyticsSolverConfig {
            flags: 1,
            ..defaults()
        },
        ItofinAnalyticsSolverConfig {
            flags: 2,
            ..defaults()
        },
        ItofinAnalyticsSolverConfig {
            flags: 8,
            ..defaults()
        },
        ItofinAnalyticsSolverConfig {
            flags: 1,
            accuracy: f64::NAN,
            ..defaults()
        },
        ItofinAnalyticsSolverConfig {
            flags: 4,
            guess: f64::INFINITY,
            ..defaults()
        },
    ] {
        assert!(solver(a).is_err());
    }
}
#[test]
fn analytics_explicit_settlement_needs_no_evaluation_engine_or_calendar_roll() {
    let (mut c, bond, leg, settings_id, dc, curve) = fixture(i32::MAX as u32);
    let actual = 46_224;
    let yield_config = ItofinBondYieldAnalyticsConfig {
        day_counter: dc,
        compounding: 1,
        frequency: 1,
        settlement: actual,
    };
    let spread_config = ItofinBondZSpreadAnalyticsConfig {
        discount: curve,
        compounding: 1,
        frequency: 1,
        settlement: actual,
    };
    let leg_config = ItofinCashFlowsZSpreadConfig {
        discount: curve,
        settings: settings_id,
        include_settlement: -1,
        settlement: actual,
        npv_date: actual - 9,
        compounding: 1,
        frequency: 1,
    };
    let mut clean = -777.0;
    let mut dirty = -777.0;
    let mut out = -777.0;
    unsafe {
        assert_eq!(
            itofin_bond_clean_price_from_curve(&mut c, bond, curve, actual, &mut out, null_mut()),
            0
        );
        assert_eq!(
            itofin_bond_dirty_price_from_curve(&mut c, bond, curve, actual, &mut out, null_mut()),
            0
        );
        assert_eq!(
            itofin_bond_clean_price_from_yield(
                &mut c,
                bond,
                0.052,
                yield_config,
                &mut clean,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_bond_dirty_price_from_yield(
                &mut c,
                bond,
                0.052,
                yield_config,
                &mut dirty,
                null_mut()
            ),
            0
        );
        for (target, kind) in [(clean, 0), (dirty, 1)] {
            assert_eq!(
                itofin_bond_yield_rate(
                    &mut c,
                    bond,
                    target,
                    kind,
                    yield_config,
                    defaults(),
                    &mut out,
                    null_mut()
                ),
                0
            );
            assert!((out - 0.052).abs() < 2e-9);
        }
        assert_eq!(
            itofin_bond_clean_price_at_z_spread(
                &mut c,
                bond,
                0.007,
                spread_config,
                &mut clean,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_bond_dirty_price_at_z_spread(
                &mut c,
                bond,
                0.007,
                spread_config,
                &mut dirty,
                null_mut()
            ),
            0
        );
        for (target, kind) in [(clean, 0), (dirty, 1)] {
            assert_eq!(
                itofin_bond_z_spread(
                    &mut c,
                    bond,
                    target,
                    kind,
                    spread_config,
                    defaults(),
                    &mut out,
                    null_mut()
                ),
                0
            );
            assert!((out - 0.007).abs() < 2e-9);
        }
        assert_eq!(
            itofin_cashflows_npv_at_z_spread(&mut c, leg, 0.007, leg_config, &mut out, null_mut()),
            0
        );
        let npv = out;
        assert_eq!(
            itofin_cashflows_z_spread(
                &mut c,
                leg,
                npv,
                leg_config,
                defaults(),
                &mut out,
                null_mut()
            ),
            0
        );
        assert!((out - 0.007).abs() < 2e-9);
        out = -777.0;
        assert_eq!(
            itofin_bond_clean_price_from_curve(&mut c, bond, curve, 0, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(out, -777.0);
    }
    let native = c.get::<NativeBond>(bond).unwrap();
    assert!(!native.bond.borrow().base().is_calculated());
    assert!(native.bond.borrow().settings().evaluation_date().is_none());
}
#[test]
fn analytics_invalid_inputs_leave_sentinel_and_context_healthy() {
    let (mut c, bond, leg, settings_id, dc, curve) = fixture(5);
    let config = ItofinBondYieldAnalyticsConfig {
        day_counter: dc,
        compounding: 1,
        frequency: 1,
        settlement: 46_224,
    };
    let spread = ItofinBondZSpreadAnalyticsConfig {
        discount: curve,
        compounding: 1,
        frequency: 1,
        settlement: 46_224,
    };
    let leg_config = ItofinCashFlowsZSpreadConfig {
        discount: curve,
        settings: settings_id,
        include_settlement: -1,
        settlement: 46_224,
        npv_date: 46_215,
        compounding: 1,
        frequency: 1,
    };
    let mut out = -777.0;
    unsafe {
        for a in [
            ItofinBondYieldAnalyticsConfig {
                compounding: -1,
                ..config
            },
            ItofinBondYieldAnalyticsConfig {
                frequency: 4,
                ..config
            },
            ItofinBondYieldAnalyticsConfig {
                settlement: -1,
                ..config
            },
            ItofinBondYieldAnalyticsConfig {
                day_counter: curve,
                ..config
            },
        ] {
            assert_ne!(
                itofin_bond_clean_price_from_yield(&mut c, bond, 0.052, a, &mut out, null_mut()),
                0
            );
            assert_eq!(out, -777.0);
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                itofin_bond_dirty_price_from_yield(
                    &mut c,
                    bond,
                    value,
                    config,
                    &mut out,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_bond_dirty_price_at_z_spread(
                    &mut c,
                    bond,
                    value,
                    spread,
                    &mut out,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_cashflows_npv_at_z_spread(
                    &mut c,
                    leg,
                    value,
                    leg_config,
                    &mut out,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(out, -777.0);
        }
        assert_eq!(
            itofin_bond_yield_rate(
                &mut c,
                bond,
                100.0,
                2,
                config,
                defaults(),
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bond_z_spread(
                &mut c,
                bond,
                100.0,
                0,
                spread,
                ItofinAnalyticsSolverConfig {
                    flags: 2,
                    ..defaults()
                },
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_cashflows_z_spread(
                &mut c,
                leg,
                375.0,
                leg_config,
                ItofinAnalyticsSolverConfig {
                    flags: 1,
                    ..defaults()
                },
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bond_dirty_price_from_curve(&mut c, leg, curve, 46_224, &mut out, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(
            itofin_bond_dirty_price_from_curve(&mut c, bond, dc, 46_224, &mut out, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(
            itofin_bond_dirty_price_from_curve(&mut c, bond, curve, 46_224, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(out, -777.0);
        let address = (&mut c as *mut Context) as usize;
        assert_eq!(
            std::thread::spawn(move || {
                let mut value = -777.0;
                let status = itofin_bond_dirty_price_from_curve(
                    address as *mut Context,
                    bond,
                    curve,
                    46_224,
                    &mut value,
                    null_mut(),
                );
                assert_eq!(value, -777.0);
                status
            })
            .join()
            .unwrap(),
            WRONG_THREAD
        );
        assert_eq!(
            itofin_bond_dirty_price_from_curve(&mut c, bond, curve, 46_224, &mut out, null_mut()),
            0
        );
        assert_eq!(itofin_handle_release(&mut c, bond, null_mut()), 0);
        out = -777.0;
        assert_eq!(
            itofin_bond_dirty_price_from_curve(&mut c, bond, curve, 46_224, &mut out, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(out, -777.0);
    }
}
#[test]
fn analytics_empty_leg_preserves_zero_and_rejects_malformed_external_values() {
    let (mut c, _, _, settings_id, _, curve) = fixture(5);
    let empty = c.insert(Leg::new()).unwrap();
    let a = ItofinCashFlowsZSpreadConfig {
        discount: curve,
        settings: settings_id,
        include_settlement: -1,
        settlement: 0,
        npv_date: 0,
        compounding: 4,
        frequency: 3,
    };
    let mut out = -777.0;
    unsafe {
        assert_eq!(
            itofin_cashflows_npv_at_z_spread(&mut c, empty, 0.007, a, &mut out, null_mut()),
            0
        );
        assert_eq!(out, 0.0);
        for bad in [
            ItofinCashFlowsZSpreadConfig {
                include_settlement: 2,
                ..a
            },
            ItofinCashFlowsZSpreadConfig {
                compounding: 5,
                ..a
            },
            ItofinCashFlowsZSpreadConfig { frequency: -1, ..a },
            ItofinCashFlowsZSpreadConfig { npv_date: -1, ..a },
            ItofinCashFlowsZSpreadConfig {
                settings: curve,
                ..a
            },
        ] {
            out = -777.0;
            assert_ne!(
                itofin_cashflows_npv_at_z_spread(&mut c, empty, 0.007, bad, &mut out, null_mut()),
                0
            );
            assert_eq!(out, -777.0);
        }
    }
}
