use libitofin::cashflows::{IborLeg, OvernightLeg};
use libitofin::handle::Handle;
use libitofin::indexes::IborIndex;
use libitofin::indexes::ibor::{Euribor, Sofr};
use libitofin::indexes::index::Index;
use libitofin::instrument::Instrument;
use libitofin::instruments::{CapFloor, CapFloorType};
use libitofin::interestrate::Compounding;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::{BachelierCapFloorEngine, BlackCapFloorEngine};
use libitofin::quotes::SimpleQuote;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::schedule::{MakeSchedule, Schedule};

struct Market {
    settings: Shared<Settings<Date>>,
    rate: Shared<SimpleQuote>,
    vol: Shared<SimpleQuote>,
    curve: Handle<dyn YieldTermStructure>,
    index: Shared<IborIndex>,
}

impl Market {
    fn new(normal: bool) -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(2, Month::January, 2026));
        let rate = shared(SimpleQuote::new(0.025));
        let vol = shared(SimpleQuote::new(if normal { 0.01 } else { 0.2 }));
        let curve = Handle::new(shared(FlatForward::new(
            Date::new(2, Month::January, 2026),
            Handle::new(rate.clone()),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let index = shared(Euribor::six_months(curve.clone(), settings.clone()));
        Self {
            settings,
            rate,
            vol,
            curve,
            index,
        }
    }

    fn schedule() -> Schedule {
        MakeSchedule::new()
            .from(Date::new(15, Month::January, 2026))
            .to(Date::new(15, Month::July, 2027))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .build()
    }

    fn instrument(&self, kind: CapFloorType, overnight: bool) -> CapFloor {
        let caps = vec![0.03, 0.035];
        let floors = vec![0.02, 0.015, 0.01];
        if overnight {
            let index = shared(Sofr::new(self.curve.clone(), self.settings.clone()));
            let coupons = OvernightLeg::new(Self::schedule(), index)
                .with_notional(100.0)
                .with_gearing(1.2)
                .with_spread(0.001)
                .coupons()
                .unwrap();
            CapFloor::from_overnight(kind, coupons, caps, floors, self.settings.clone()).unwrap()
        } else {
            let coupons = IborLeg::new(Self::schedule(), self.index.clone())
                .with_notional(100.0)
                .with_gearing(1.2)
                .with_spread(0.001)
                .coupons()
                .unwrap();
            CapFloor::new(kind, coupons, caps, floors, self.settings.clone()).unwrap()
        }
    }

    fn engine(&self, normal: bool) -> SharedMut<dyn PricingEngine> {
        if normal {
            shared_mut(
                BachelierCapFloorEngine::with_flat_vol(
                    self.curve.clone(),
                    Handle::new(self.vol.clone()),
                    Actual365Fixed::new(),
                    self.settings.clone(),
                )
                .unwrap(),
            )
        } else {
            shared_mut(
                BlackCapFloorEngine::with_flat_vol(
                    self.curve.clone(),
                    Handle::new(self.vol.clone()),
                    Actual365Fixed::new(),
                    0.02,
                    self.settings.clone(),
                )
                .unwrap(),
            )
        }
    }
}

fn kinds() -> [CapFloorType; 3] {
    [CapFloorType::Cap, CapFloorType::Floor, CapFloorType::Collar]
}

fn assert_decomposition(parent: &mut CapFloor, parts: &mut [CapFloor]) -> f64 {
    let value = parent.npv().unwrap();
    let sum: f64 = parts.iter_mut().map(|part| part.npv().unwrap()).sum();
    assert!(
        (value - sum).abs() < 1e-12,
        "parent {value}, optionlets {sum}"
    );
    value
}

#[test]
fn extracts_both_coupon_types_with_only_relevant_padded_strikes_and_no_engine() {
    let market = Market::new(true);
    for overnight in [false, true] {
        for kind in kinds() {
            let mut parent = market.instrument(kind, overnight);
            parent.base_mut().set_pricing_engine(market.engine(true));
            parent.npv().unwrap();
            assert_eq!(parent.coupon_count(), 3);
            for index in 0..parent.coupon_count() {
                let mut part = parent.optionlet(index).unwrap();
                assert_eq!(part.cap_floor_type(), kind);
                assert_eq!(part.coupon_count(), 1);
                assert!(part.base().pricing_engine().is_none());
                assert!(part.npv().is_err());
                if kind != CapFloorType::Floor {
                    assert_eq!(part.cap_rates(), &[parent.cap_rates()[index]]);
                } else {
                    assert!(part.cap_rates().is_empty());
                }
                if kind != CapFloorType::Cap {
                    assert_eq!(part.floor_rates(), &[parent.floor_rates()[index]]);
                } else {
                    assert!(part.floor_rates().is_empty());
                }
                if overnight {
                    assert!(part.coupons().is_empty());
                    assert!(Shared::ptr_eq(
                        &part.overnight_coupons()[0],
                        &parent.overnight_coupons()[index]
                    ));
                } else {
                    assert!(part.overnight_coupons().is_empty());
                    assert!(Shared::ptr_eq(&part.coupons()[0], &parent.coupons()[index]));
                }
            }
            assert!(parent.optionlet(3).is_err());
            assert!(parent.optionlet(usize::MAX).is_err());
        }
    }
    let empty = CapFloor::cap(Vec::new(), vec![0.03], market.settings.clone()).unwrap();
    assert!(empty.optionlet(0).is_err());
    let empty = CapFloor::from_overnight(
        CapFloorType::Floor,
        Vec::new(),
        Vec::new(),
        vec![0.01],
        market.settings,
    )
    .unwrap();
    assert!(empty.optionlet(0).is_err());
}

#[test]
fn nonflat_optionlets_sum_to_parent_and_reprice_after_market_and_date_updates() {
    for normal in [false, true] {
        for overnight in [false, true] {
            for kind in kinds() {
                let market = Market::new(normal);
                let mut parent = market.instrument(kind, overnight);
                let mut parts: Vec<_> = (0..3).map(|i| parent.optionlet(i).unwrap()).collect();
                let engine = market.engine(normal);
                parent.base_mut().set_pricing_engine(engine.clone());
                for part in &mut parts {
                    part.base_mut().set_pricing_engine(engine.clone());
                }
                let original = assert_decomposition(&mut parent, &mut parts);
                market.vol.set_value(if normal { 0.013 } else { 0.3 });
                let changed_vol = assert_decomposition(&mut parent, &mut parts);
                assert!((original - changed_vol).abs() > 1e-8);
                market.rate.set_value(0.032);
                let changed_rate = assert_decomposition(&mut parent, &mut parts);
                assert!((changed_vol - changed_rate).abs() > 1e-8);
                market
                    .settings
                    .set_evaluation_date(Date::new(5, Month::January, 2026));
                let changed_date = assert_decomposition(&mut parent, &mut parts);
                assert!((changed_rate - changed_date).abs() > 1e-8);
                market
                    .settings
                    .set_evaluation_date(Date::new(16, Month::July, 2027));
                assert_eq!(assert_decomposition(&mut parent, &mut parts), 0.0);
                assert!(parts.iter().all(|part| part.is_expired().unwrap()));
            }
        }
    }
}

#[test]
fn extracted_ibor_optionlet_retains_shared_historical_fixings() {
    let market = Market::new(true);
    let mut parent = market.instrument(CapFloorType::Cap, false);
    let mut part = parent.optionlet(0).unwrap();
    let fixing_date = part.coupons()[0].fixing_date();
    market
        .settings
        .set_evaluation_date(Date::new(16, Month::January, 2026));
    market.index.add_fixing(fixing_date, 0.04).unwrap();
    let engine = market.engine(true);
    parent.base_mut().set_pricing_engine(engine.clone());
    part.base_mut().set_pricing_engine(engine);
    let before = part.npv().unwrap();
    market.settings.clear_fixings();
    market.index.add_fixing(fixing_date, 0.05).unwrap();
    let after = part.npv().unwrap();
    assert!(after > before);
    let mut remaining: Vec<_> = (1..3).map(|i| parent.optionlet(i).unwrap()).collect();
    for part in &mut remaining {
        part.base_mut().set_pricing_engine(market.engine(true));
    }
    assert!(
        (parent.npv().unwrap()
            - after
            - remaining.iter_mut().map(|p| p.npv().unwrap()).sum::<f64>())
        .abs()
            < 1e-12
    );
}

#[test]
fn one_coupon_prices_match_independent_quantlib_143_fixtures() {
    let data = include_str!("fixtures/capfloor-optionlets/quantlib-1.43.csv");
    for line in data.lines().skip(1) {
        let cells: Vec<_> = line.split(',').collect();
        let overnight = cells[0].parse::<bool>().unwrap();
        let normal = cells[1].parse::<bool>().unwrap();
        let kind = match cells[2] {
            "cap" => CapFloorType::Cap,
            "floor" => CapFloorType::Floor,
            "collar" => CapFloorType::Collar,
            other => panic!("unknown fixture kind {other}"),
        };
        let market = Market::new(normal);
        let mut parent = market.instrument(kind, overnight);
        parent.base_mut().set_pricing_engine(market.engine(normal));
        for (index, expected) in cells[4..7].iter().enumerate() {
            let mut part = parent.optionlet(index).unwrap();
            part.base_mut().set_pricing_engine(market.engine(normal));
            let actual = part.npv().unwrap();
            let expected = expected.parse::<f64>().unwrap();
            assert!(
                (actual - expected).abs() < 1e-12,
                "{line}: optionlet {index}: actual {actual}, expected {expected}"
            );
        }
        let actual = parent.npv().unwrap();
        let expected = cells[3].parse::<f64>().unwrap();
        assert!(
            (actual - expected).abs() < 1e-12,
            "{line}: parent: actual {actual}, expected {expected}"
        );
    }
}

#[test]
fn extracted_overnight_optionlet_observes_shared_daily_fixing_history() {
    let market = Market::new(true);
    let parent = market.instrument(CapFloorType::Cap, true);
    let mut part = parent.optionlet(0).unwrap();
    let coupon = part.overnight_coupons()[0].clone();
    let fixing = coupon.fixing_dates()[0];
    let index = coupon.overnight_index();
    market
        .settings
        .set_evaluation_date(Date::new(16, Month::January, 2026));
    index.add_fixing(fixing, 0.04).unwrap();
    part.base_mut().set_pricing_engine(market.engine(true));
    let before = part.npv().unwrap();
    market.settings.clear_fixings();
    index.add_fixing(fixing, 0.05).unwrap();
    let after = part.npv().unwrap();
    assert!(after > before);
    let mut same = parent.optionlet(0).unwrap();
    same.base_mut().set_pricing_engine(market.engine(true));
    assert!((same.npv().unwrap() - after).abs() < 1e-12);
}
