use super::*;
use crate::calendar_api::NativeCalendar;
use libitofin::instrument::Instrument;
use libitofin::shared::{shared, shared_mut};
use libitofin::time::calendars::NullCalendar;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use std::cell::Cell;
use std::ptr::null_mut;

#[path = "../../libitofin/tests/fixtures/medium6_callable_sensitivities/support.rs"]
mod native;

fn fixture(
    kind: &str,
    right: &str,
    redemption: f64,
) -> (Context, u64, ItofinCallableBondRiskConfig) {
    let mut c = Context::new();
    let settings = native::settings();
    let calls = native::rights(right);
    let backing = if kind == "fixed" {
        BondBacking::CallableFixed(shared_mut(native::fixed(settings.clone(), calls)))
    } else {
        BondBacking::CallableZero(shared_mut(native::zero(
            settings.clone(),
            calls,
            redemption,
        )))
    };
    let flags = shared(Cell::new((false, None)));
    let bond = c
        .insert(NativeBond {
            bond: backing,
            calendar: NativeCalendar {
                inner: NullCalendar::new(),
                horizon: None,
                first_year: 1901,
            },
            fixed: None,
            flags,
            floating: None,
        })
        .unwrap();
    let tree_engine = c
        .insert(NativeCallableTreeEngine {
            engine: shared_mut(native::engine(settings.clone())),
            settings,
        })
        .unwrap();
    let day_counter = c.insert(Actual365Fixed::new()).unwrap();
    (
        c,
        bond,
        ItofinCallableBondRiskConfig {
            tree_engine,
            day_counter,
            compounding: 2,
            frequency: 0,
            flags: 0,
            bump: f64::NAN,
        },
    )
}

#[test]
fn native_quantlib143_risk_fixtures_and_direct_core_identity() {
    let rows = include_str!("../../../sdk/go/testdata/callable_risk_native.tsv");
    assert_eq!(rows.lines().count(), 37);
    for row in rows.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        let (mut c, id, mut a) = fixture(fields[0], fields[1], 100.0);
        a.compounding = match fields[2] {
            "continuous" => 2,
            "compounded" => 1,
            "simple" => 0,
            _ => unreachable!(),
        };
        a.frequency = i32::from(a.compounding == 1);
        a.flags = 1;
        a.bump = fields[3].parse().unwrap();
        for (convexity, index, tolerance) in [(false, 8, 2e-9), (true, 9, 2e-6)] {
            let got = risk(&c, id, 0.0075, a, convexity).unwrap();
            native::close(got, fields[index].parse().unwrap(), tolerance);
            let bond = c.get::<NativeBond>(id).unwrap();
            let engine = c.get::<NativeCallableTreeEngine>(a.tree_engine).unwrap();
            let dc = Actual365Fixed::new();
            let comp = compounding(a.compounding).unwrap();
            let freq = frequency(a.frequency).unwrap();
            let direct = match &bond.bond {
                BondBacking::CallableFixed(bond) => {
                    let bond = bond.borrow();
                    if convexity {
                        bond.effective_convexity(
                            0.0075,
                            &engine.engine.borrow(),
                            dc,
                            comp,
                            freq,
                            Some(a.bump),
                        )
                    } else {
                        bond.effective_duration(
                            0.0075,
                            &engine.engine.borrow(),
                            dc,
                            comp,
                            freq,
                            Some(a.bump),
                        )
                    }
                }
                BondBacking::CallableZero(bond) => {
                    let bond = bond.borrow();
                    if convexity {
                        bond.effective_convexity(
                            0.0075,
                            &engine.engine.borrow(),
                            dc,
                            comp,
                            freq,
                            Some(a.bump),
                        )
                    } else {
                        bond.effective_duration(
                            0.0075,
                            &engine.engine.borrow(),
                            dc,
                            comp,
                            freq,
                            Some(a.bump),
                        )
                    }
                }
                _ => unreachable!(),
            }
            .unwrap();
            assert_eq!(got, direct);
            assert!(!bond.bond.borrow().base().is_calculated());
            assert!(bond.bond.borrow().base().pricing_engine().is_none());
            assert_eq!(bond.flags.get(), (false, None));
            let mut output = -999.0;
            let function = if convexity {
                itofin_callable_bond_effective_convexity
            } else {
                itofin_callable_bond_effective_duration
            };
            assert_eq!(
                unsafe { function(&mut c, id, 0.0075, a, &mut output, null_mut()) },
                0
            );
            assert_eq!(output, got);
            if a.bump == 2e-4 {
                let absent = ItofinCallableBondRiskConfig {
                    flags: 0,
                    bump: f64::NAN,
                    ..a
                };
                assert_eq!(risk(&c, id, 0.0075, absent, convexity).unwrap(), got);
            }
        }
    }
}

#[test]
fn boundary_errors_preserve_sentinels_cache_engine_and_silent_flags() {
    for kind in ["fixed", "zero"] {
        let (mut c, id, a) = fixture(kind, "call", 100.0);
        let bond = c.get::<NativeBond>(id).unwrap();
        let engine = c.get::<NativeCallableTreeEngine>(a.tree_engine).unwrap();
        bond.bond
            .with_instrument_mut(|b| b.base_mut().set_pricing_engine(engine.engine.clone()));
        let cached = bond.bond.real(0, native::reference()).unwrap();
        bond.bond.with_instrument_mut(|b| b.base_mut().freeze());
        engine.settings.set_include_reference_date_events(true);
        let wrong_settings = native::settings();
        let wrong = c
            .insert(NativeCallableTreeEngine {
                engine: engine.engine.clone(),
                settings: wrong_settings,
            })
            .unwrap();
        let bad = [
            ItofinCallableBondRiskConfig { flags: 2, ..a },
            ItofinCallableBondRiskConfig {
                tree_engine: wrong,
                ..a
            },
            ItofinCallableBondRiskConfig {
                tree_engine: id,
                ..a
            },
            ItofinCallableBondRiskConfig {
                day_counter: id,
                ..a
            },
            ItofinCallableBondRiskConfig {
                compounding: 5,
                ..a
            },
            ItofinCallableBondRiskConfig { frequency: 4, ..a },
            ItofinCallableBondRiskConfig {
                flags: 1,
                bump: 0.0,
                ..a
            },
            ItofinCallableBondRiskConfig {
                flags: 1,
                bump: -1.0,
                ..a
            },
            ItofinCallableBondRiskConfig {
                flags: 1,
                bump: f64::INFINITY,
                ..a
            },
            ItofinCallableBondRiskConfig {
                flags: 1,
                bump: f64::NAN,
                ..a
            },
            ItofinCallableBondRiskConfig {
                flags: 1,
                bump: 1e-320,
                ..a
            },
        ];
        for function in [
            itofin_callable_bond_effective_duration,
            itofin_callable_bond_effective_convexity,
        ] {
            for config in bad {
                let mut output = -999.0;
                assert_ne!(
                    unsafe { function(&mut c, id, 0.0075, config, &mut output, null_mut()) },
                    0
                );
                assert_eq!(output, -999.0);
            }
            assert_ne!(
                unsafe { function(&mut c, id, 0.0075, a, null_mut(), null_mut()) },
                0
            );
            for spread in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut output = -999.0;
                assert_ne!(
                    unsafe { function(&mut c, id, spread, a, &mut output, null_mut()) },
                    0
                );
                assert_eq!(output, -999.0);
            }
        }
        for convexity in [false, true] {
            assert!(risk(&c, id, 0.0075, a, convexity).is_ok());
        }
        let base = bond.bond.borrow();
        assert!(base.base().is_calculated());
        assert_eq!(base.base().results().value, Some(cached));
        assert!(libitofin::shared::SharedMut::ptr_eq(
            base.base().pricing_engine().unwrap(),
            &(engine.engine.clone()
                as libitofin::shared::SharedMut<dyn libitofin::pricingengine::PricingEngine>)
        ));
        assert_eq!(bond.flags.get(), (false, None));
    }
}

#[test]
fn zero_center_checks_both_native_bump_prices_and_plain_products_reject() {
    let (mut c, id, mut a) = fixture("zero", "none", 0.0);
    for convexity in [false, true] {
        assert_eq!(risk(&c, id, 0.0, a, convexity).unwrap(), 0.0);
        a.compounding = 0;
        a.flags = 1;
        a.bump = 10.0;
        assert!(risk(&c, id, 0.0, a, convexity).is_err());
        a.compounding = 2;
        a.flags = 0;
    }
    let plain = c
        .insert(NativeBond::new_unpriced(
            libitofin::instruments::ZeroCouponBond::new(
                0,
                NullCalendar::new(),
                100.0,
                native::maturity(),
                libitofin::time::businessdayconvention::BusinessDayConvention::Unadjusted,
                100.0,
                None,
                native::settings(),
            )
            .unwrap()
            .into_bond(),
            NativeCalendar {
                inner: NullCalendar::new(),
                horizon: None,
                first_year: 1901,
            },
            None,
        ))
        .unwrap();
    assert!(risk(&c, plain, 0.0, a, false).is_err());
}

#[test]
fn wrong_thread_and_foreign_context_keep_output_untouched() {
    let (mut c, id, a) = fixture("zero", "none", 100.0);
    let address = &mut c as *mut Context as usize;
    let code = std::thread::spawn(move || {
        let mut output = -999.0;
        let code = unsafe {
            itofin_callable_bond_effective_duration(
                address as *mut Context,
                id,
                0.0,
                a,
                &mut output,
                null_mut(),
            )
        };
        assert_eq!(output, -999.0);
        code
    })
    .join()
    .unwrap();
    assert_eq!(code, WRONG_THREAD);
    let mut output = -999.0;
    assert_eq!(
        unsafe {
            itofin_callable_bond_effective_convexity(
                &mut Context::new(),
                id,
                0.0,
                a,
                &mut output,
                null_mut(),
            )
        },
        INVALID_HANDLE
    );
    assert_eq!(output, -999.0);
    let bond = c.get::<NativeBond>(id).unwrap();
    bond.bond.with_instrument_mut(|b| b.base_mut().freeze());
    assert!(risk(&c, id, 0.0, a, false).is_ok());
    assert!(!bond.bond.borrow().base().is_calculated());
    assert!(bond.bond.borrow().base().pricing_engine().is_none());
}
