//! Retained hard call/put schedules and native callable bond owners.

use crate::bonds::{BondBacking, PyBond, adjust, advance, finite, positive};
use crate::curve::PyYieldTermStructure;
use crate::helpers::PyBondPriceType;
use crate::hullwhite::PyHullWhite;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyCompounding, PyDate, PyDayCounter, PyFrequency,
    PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    BondPrice, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond,
};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::{
    BlackCallableFixedRateBondEngine, TreeCallableFixedRateBondEngine,
};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::time::date::Date;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};

/// Issuer redemption right or holder sale right.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "CallabilityType",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.instruments"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyCallabilityType {
    Call = 0,
    Put = 1,
}
impl PyCallabilityType {
    fn inner(self) -> CallabilityType {
        match self {
            Self::Call => CallabilityType::Call,
            Self::Put => CallabilityType::Put,
        }
    }
}

/// Immutable hard exercise right with a clean or dirty per-100 price.
#[gen_stub_pyclass]
#[pyclass(name = "Callability", from_py_object, module = "itofin.instruments")]
#[derive(Clone)]
pub struct PyCallability {
    inner: Callability,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCallability {
    #[new]
    #[pyo3(signature=(price, call_type, date, price_type=PyBondPriceType::Clean))]
    fn new(
        price: f64,
        call_type: PyCallabilityType,
        date: &PyDate,
        price_type: PyBondPriceType,
    ) -> PyResult<Self> {
        let price = match price_type {
            PyBondPriceType::Clean => BondPrice::Clean(price),
            PyBondPriceType::Dirty => BondPrice::Dirty(price),
        };
        Ok(Self {
            inner: Callability::new(price, call_type.inner(), date.inner())
                .map_err(PyQlError::from)?,
        })
    }
    fn price(&self) -> f64 {
        self.inner.price().amount()
    }
    fn price_type(&self) -> PyBondPriceType {
        match self.inner.price() {
            BondPrice::Clean(_) => PyBondPriceType::Clean,
            BondPrice::Dirty(_) => PyBondPriceType::Dirty,
        }
    }
    fn call_type(&self) -> PyCallabilityType {
        match self.inner.call_type() {
            CallabilityType::Call => PyCallabilityType::Call,
            CallabilityType::Put => PyCallabilityType::Put,
        }
    }
    fn date(&self) -> PyDate {
        PyDate::from_inner(self.inner.date())
    }
}
fn amounts(
    settlement_days: u32,
    face_amount: f64,
    redemption: f64,
    issue: Option<&PyDate>,
) -> PyResult<()> {
    if settlement_days > (Date::max_date() - Date::min_date()) as u32 {
        return Err(ItofinError::new_err(
            "settlement days exceed supported date range",
        ));
    }
    positive(face_amount)?;
    finite(redemption)?;
    finite(face_amount * redemption / 100.0)?;
    if redemption < 0.0 {
        return Err(ItofinError::new_err("redemption must be nonnegative"));
    }
    if issue.is_some_and(|date| date.inner() == Date::null()) {
        return Err(ItofinError::new_err("null issue date"));
    }
    Ok(())
}

/// Callable fixed bond sharing its actual native lazy base with Bond methods.
#[gen_stub_pyclass]
#[pyclass(name="CallableFixedRateBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyCallableFixedRateBond {
    inner: libitofin::shared::SharedMut<CallableFixedRateBond>,
    frequency: Frequency,
    day_counter: PyDayCounter,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCallableFixedRateBond {
    #[new]
    #[gen_stub(override_return_type(type_repr = "CallableFixedRateBond"))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, face_amount, schedule, coupons, accrual_day_counter, callability, settings, *, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false))]
    fn new(
        settlement_days: u32,
        face_amount: f64,
        schedule: &PySchedule,
        coupons: Vec<f64>,
        accrual_day_counter: &PyDayCounter,
        callability: Vec<PyCallability>,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_convention: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
    ) -> PyResult<PyClassInitializer<Self>> {
        amounts(settlement_days, face_amount, redemption, issue_date)?;
        if ex_coupon_period.is_some_and(|period| period.inner().length() < 0) {
            return Err(ItofinError::new_err("ex-coupon period must be nonnegative"));
        }
        for &coupon in &coupons {
            finite(coupon)?;
        }
        let calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        let ex_calendar = ex_coupon_calendar.cloned().unwrap_or_else(|| {
            PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new())
        });
        for &date in schedule.dates().iter().skip(1) {
            let payment = adjust(&calendar, date, payment_convention.inner())?;
            if let Some(period) = ex_coupon_period {
                let period = period.inner();
                advance(
                    &ex_calendar,
                    payment,
                    -i64::from(period.length()),
                    period.units(),
                    ex_coupon_convention.inner(),
                    ex_coupon_end_of_month,
                )?;
            }
        }
        if schedule.has_tenor() && schedule.has_is_regular() && schedule.has_end_of_month() {
            let tenor = schedule.tenor();
            if !schedule.is_regular_at(1) {
                advance(
                    &calendar,
                    schedule.date(1),
                    -i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
            if schedule.len() > 2 && !schedule.is_regular_at(schedule.len() - 1) {
                advance(
                    &calendar,
                    schedule.date(schedule.len() - 2),
                    i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
        }
        let frequency = if schedule.has_tenor() {
            schedule.tenor().frequency()
        } else {
            Frequency::NoFrequency
        };
        let bond = CallableFixedRateBond::with_ex_coupon(
            settlement_days,
            face_amount,
            schedule,
            coupons,
            accrual_day_counter.inner(),
            payment_convention.inner(),
            redemption,
            issue_date.map(PyDate::inner),
            callability.into_iter().map(|call| call.inner).collect(),
            ex_coupon_period.map(PyPeriod::inner),
            ex_calendar.inner(),
            ex_coupon_convention.inner(),
            ex_coupon_end_of_month,
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        for flow in bond.bond().cashflows() {
            finite(flow.amount().map_err(PyQlError::from)?)?;
        }
        let inner = shared_mut(bond);
        Ok(PyClassInitializer::from(PyBond::from_backing(
            BondBacking::CallableFixed(inner.clone()),
            calendar,
        ))
        .add_subclass(Self {
            inner,
            frequency,
            day_counter: PyDayCounter::from_inner(accrual_day_counter.inner()),
        }))
    }
    /// Attach a retained Hull-White tree engine sharing this bond's settings.
    fn set_tree_engine(&mut self, engine: &PyTreeCallableFixedRateBondEngine) -> PyResult<()> {
        if !Shared::ptr_eq(
            &self.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
        Ok(())
    }
    /// Attach a retained single-European Black engine sharing this bond's settings.
    fn set_black_engine(&mut self, engine: &PyBlackCallableFixedRateBondEngine) -> PyResult<()> {
        if !Shared::ptr_eq(
            &self.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
        Ok(())
    }
    /// Clean settlement quote per 100 from a conventional decimal OAS and explicit tree.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(spread, engine, day_counter, compounding, frequency, settlement=None))]
    fn clean_price_oas(
        self_: PyRef<'_, Self>,
        spread: f64,
        engine: &PyTreeCallableFixedRateBondEngine,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        finite(spread)?;
        if !Shared::ptr_eq(
            &self_.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        let settlement = self_.as_super().callable_probe_settlement(settlement)?;
        finite(
            self_
                .inner
                .borrow()
                .clean_price_oas(
                    spread,
                    &engine.inner.borrow(),
                    day_counter.inner(),
                    compounding.inner(),
                    frequency.inner(),
                    settlement,
                )
                .map_err(PyQlError::from)?,
        )
    }
    /// Solve conventional decimal OAS; accuracy and guess are continuous shift units.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(clean_price, engine, day_counter, compounding, frequency, settlement=None, *, accuracy=1e-10, max_evaluations=100, guess=0.0))]
    fn oas(
        self_: PyRef<'_, Self>,
        clean_price: f64,
        engine: &PyTreeCallableFixedRateBondEngine,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: PyFrequency,
        settlement: Option<&PyDate>,
        accuracy: f64,
        max_evaluations: usize,
        guess: f64,
    ) -> PyResult<f64> {
        finite(clean_price)?;
        crate::bonds::analytical_solver(accuracy, max_evaluations, guess)?;
        if !Shared::ptr_eq(
            &self_.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        let settlement = self_.as_super().callable_probe_settlement(settlement)?;
        finite(
            self_
                .inner
                .borrow()
                .oas(
                    clean_price,
                    &engine.inner.borrow(),
                    day_counter.inner(),
                    compounding.inner(),
                    frequency.inner(),
                    settlement,
                    accuracy,
                    max_evaluations,
                    guess,
                )
                .map_err(PyQlError::from)?,
        )
    }
    /// Solve relative forward-yield volatility without changing installed pricing state.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(target_price, discount_curve, *, price_type=PyBondPriceType::Clean, accuracy=1e-10, max_evaluations=100, min_vol=0.0, max_vol=1.0))]
    fn implied_volatility(
        self_: PyRef<'_, Self>,
        target_price: f64,
        discount_curve: &PyYieldTermStructure,
        price_type: PyBondPriceType,
        accuracy: f64,
        max_evaluations: usize,
        min_vol: f64,
        max_vol: f64,
    ) -> PyResult<f64> {
        self_.as_super().callable_implied_volatility(
            target_price,
            discount_curve,
            price_type,
            accuracy,
            max_evaluations,
            min_vol,
            max_vol,
        )
    }
    fn callability(&self) -> Vec<PyCallability> {
        self.inner
            .borrow()
            .callability()
            .iter()
            .cloned()
            .map(|inner| PyCallability { inner })
            .collect()
    }
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(self.frequency)
    }
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.day_counter.inner())
    }
}

/// Callable single-redemption bond retaining its native day counter and base.
#[gen_stub_pyclass]
#[pyclass(name="CallableZeroCouponBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyCallableZeroCouponBond {
    inner: libitofin::shared::SharedMut<CallableZeroCouponBond>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCallableZeroCouponBond {
    #[new]
    #[gen_stub(override_return_type(type_repr = "CallableZeroCouponBond"))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, calendar, face_amount, maturity_date, day_counter, callability, settings, *, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None))]
    fn new(
        settlement_days: u32,
        calendar: &PyCalendar,
        face_amount: f64,
        maturity_date: &PyDate,
        day_counter: &PyDayCounter,
        callability: Vec<PyCallability>,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
    ) -> PyResult<PyClassInitializer<Self>> {
        amounts(settlement_days, face_amount, redemption, issue_date)?;
        adjust(calendar, maturity_date.inner(), payment_convention.inner())?;
        let inner = shared_mut(
            CallableZeroCouponBond::new(
                settlement_days,
                face_amount,
                calendar.inner(),
                maturity_date.inner(),
                day_counter.inner(),
                payment_convention.inner(),
                redemption,
                issue_date.map(PyDate::inner),
                callability.into_iter().map(|call| call.inner).collect(),
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        Ok(PyClassInitializer::from(PyBond::from_backing(
            BondBacking::CallableZero(inner.clone()),
            calendar.clone(),
        ))
        .add_subclass(Self { inner }))
    }
    /// Attach a retained Hull-White tree engine sharing this bond's settings.
    fn set_tree_engine(&mut self, engine: &PyTreeCallableFixedRateBondEngine) -> PyResult<()> {
        if !Shared::ptr_eq(
            &self.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
        Ok(())
    }
    /// Attach a retained single-European Black engine sharing this bond's settings.
    fn set_black_engine(&mut self, engine: &PyBlackCallableFixedRateBondEngine) -> PyResult<()> {
        if !Shared::ptr_eq(
            &self.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
        Ok(())
    }
    /// Clean settlement quote per 100 from a conventional decimal OAS and explicit tree.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(spread, engine, day_counter, compounding, frequency, settlement=None))]
    fn clean_price_oas(
        self_: PyRef<'_, Self>,
        spread: f64,
        engine: &PyTreeCallableFixedRateBondEngine,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: PyFrequency,
        settlement: Option<&PyDate>,
    ) -> PyResult<f64> {
        finite(spread)?;
        if !Shared::ptr_eq(
            &self_.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        let settlement = self_.as_super().callable_probe_settlement(settlement)?;
        finite(
            self_
                .inner
                .borrow()
                .clean_price_oas(
                    spread,
                    &engine.inner.borrow(),
                    day_counter.inner(),
                    compounding.inner(),
                    frequency.inner(),
                    settlement,
                )
                .map_err(PyQlError::from)?,
        )
    }
    /// Solve conventional decimal OAS; accuracy and guess are continuous shift units.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(clean_price, engine, day_counter, compounding, frequency, settlement=None, *, accuracy=1e-10, max_evaluations=100, guess=0.0))]
    fn oas(
        self_: PyRef<'_, Self>,
        clean_price: f64,
        engine: &PyTreeCallableFixedRateBondEngine,
        day_counter: &PyDayCounter,
        compounding: PyCompounding,
        frequency: PyFrequency,
        settlement: Option<&PyDate>,
        accuracy: f64,
        max_evaluations: usize,
        guess: f64,
    ) -> PyResult<f64> {
        finite(clean_price)?;
        crate::bonds::analytical_solver(accuracy, max_evaluations, guess)?;
        if !Shared::ptr_eq(
            &self_.inner.borrow().bond().settings_handle(),
            &engine.settings,
        ) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        let settlement = self_.as_super().callable_probe_settlement(settlement)?;
        finite(
            self_
                .inner
                .borrow()
                .oas(
                    clean_price,
                    &engine.inner.borrow(),
                    day_counter.inner(),
                    compounding.inner(),
                    frequency.inner(),
                    settlement,
                    accuracy,
                    max_evaluations,
                    guess,
                )
                .map_err(PyQlError::from)?,
        )
    }
    /// Solve relative forward-yield volatility without changing installed pricing state.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(target_price, discount_curve, *, price_type=PyBondPriceType::Clean, accuracy=1e-10, max_evaluations=100, min_vol=0.0, max_vol=1.0))]
    fn implied_volatility(
        self_: PyRef<'_, Self>,
        target_price: f64,
        discount_curve: &PyYieldTermStructure,
        price_type: PyBondPriceType,
        accuracy: f64,
        max_evaluations: usize,
        min_vol: f64,
        max_vol: f64,
    ) -> PyResult<f64> {
        self_.as_super().callable_implied_volatility(
            target_price,
            discount_curve,
            price_type,
            accuracy,
            max_evaluations,
            min_vol,
            max_vol,
        )
    }
    fn callability(&self) -> Vec<PyCallability> {
        self.inner
            .borrow()
            .callability()
            .iter()
            .cloned()
            .map(|inner| PyCallability { inner })
            .collect()
    }
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.inner.borrow().day_counter().clone())
    }
}

/// Event-aligned callable fixed/zero bond pricing on the retained Hull-White model.
#[gen_stub_pyclass]
#[pyclass(
    name = "TreeCallableFixedRateBondEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyTreeCallableFixedRateBondEngine {
    inner: SharedMut<TreeCallableFixedRateBondEngine>,
    settings: Shared<Settings<Date>>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyTreeCallableFixedRateBondEngine {
    /// Retain the existing model and settings; time steps must be positive.
    #[new]
    fn new(model: &PyHullWhite, time_steps: usize, settings: &PySettings) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                TreeCallableFixedRateBondEngine::new(model.inner(), time_steps, settings.inner())
                    .map_err(PyQlError::from)?,
            ),
            settings: settings.inner(),
        })
    }
}
impl PyTreeCallableFixedRateBondEngine {
    fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}

/// Single-European constant relative forward-yield volatility pricing.
#[gen_stub_pyclass]
#[pyclass(
    name = "BlackCallableFixedRateBondEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyBlackCallableFixedRateBondEngine {
    inner: SharedMut<BlackCallableFixedRateBondEngine>,
    settings: Shared<Settings<Date>>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyBlackCallableFixedRateBondEngine {
    /// Retain the live volatility quote, discount curve and exact settings allocation.
    #[new]
    fn new(
        yield_volatility: &PySimpleQuote,
        discount_curve: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> Self {
        Self {
            inner: shared_mut(BlackCallableFixedRateBondEngine::new(
                yield_volatility.handle(),
                discount_curve.handle(),
                settings.inner(),
            )),
            settings: settings.inner(),
        }
    }
}
impl PyBlackCallableFixedRateBondEngine {
    fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}

#[cfg(test)]
mod tree_tests {
    use super::*;
    use libitofin::handle::{Handle, RelinkableHandle};
    use libitofin::interestrate::Compounding;
    use libitofin::math::array::Array;
    use libitofin::models::model::CalibratedModelHolder;
    use libitofin::models::shortrate::HullWhite;
    use libitofin::patterns::observable::Observer;
    use libitofin::quotes::{Quote, SimpleQuote};
    use libitofin::shared::shared;
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    use libitofin::time::businessdayconvention::BusinessDayConvention;
    use libitofin::time::calendars::NullCalendar;
    use libitofin::time::date::Month;
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use std::cell::Cell;

    struct Notifications(Shared<Cell<usize>>);
    impl Observer for Notifications {
        fn update(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    fn reference() -> Date {
        Date::new(7, Month::July, 2026)
    }
    fn flat(rate: f64) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            reference(),
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    }
    fn product(settings: Shared<Settings<Date>>) -> PyCallableZeroCouponBond {
        let call = Callability::new(
            BondPrice::Clean(95.25),
            CallabilityType::Put,
            Date::new(7, Month::July, 2028),
        )
        .unwrap();
        PyCallableZeroCouponBond {
            inner: shared_mut(
                CallableZeroCouponBond::new(
                    2,
                    1000.0,
                    NullCalendar::new(),
                    Date::new(7, Month::July, 2031),
                    Actual365Fixed::new(),
                    BusinessDayConvention::Unadjusted,
                    100.0,
                    None,
                    vec![call],
                    settings,
                )
                .unwrap(),
            ),
        }
    }
    fn engine(
        model: SharedMut<HullWhite>,
        settings: Shared<Settings<Date>>,
    ) -> PyTreeCallableFixedRateBondEngine {
        PyTreeCallableFixedRateBondEngine {
            inner: shared_mut(
                TreeCallableFixedRateBondEngine::new(model, 100, Shared::clone(&settings)).unwrap(),
            ),
            settings,
        }
    }
    #[test]
    fn typed_tree_attachment_observes_native_model_relink_and_detaches_old_engine() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference());
        let quote = shared(SimpleQuote::new(0.03));
        let initial: Shared<dyn YieldTermStructure> = shared(FlatForward::new(
            reference(),
            Handle::new(quote.clone() as Shared<dyn Quote>),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        let curve = RelinkableHandle::new(initial);
        let model = HullWhite::new(curve.handle().clone(), 0.1, 0.01).unwrap();
        let original = engine(model.clone(), settings.clone());
        let mut bond = product(settings.clone());
        bond.set_tree_engine(&original).unwrap();
        let first = bond.inner.borrow_mut().clean_price().unwrap();
        quote.set_value(0.04);
        assert!(!bond.inner.borrow().base().is_calculated());
        let quote_price = bond.inner.borrow_mut().clean_price().unwrap();
        assert!((first - quote_price).abs() > 0.01);
        quote.set_value(f64::NAN);
        assert!(bond.inner.borrow_mut().npv().is_err());
        quote.set_value(0.04);
        assert!((bond.inner.borrow_mut().clean_price().unwrap() - quote_price).abs() <= 1e-11);
        curve.link_to(flat(0.025));
        assert!(!bond.inner.borrow().base().is_calculated());
        model
            .borrow_mut()
            .set_params(&Array::from([0.1, 0.02]))
            .unwrap();
        assert!(!bond.inner.borrow().base().is_calculated());
        let current = bond.inner.borrow_mut().clean_price().unwrap();
        let mut fresh = product(settings.clone());
        fresh
            .set_tree_engine(&engine(
                HullWhite::new(Handle::new(flat(0.025)), 0.1, 0.02).unwrap(),
                settings.clone(),
            ))
            .unwrap();
        assert!((current - fresh.inner.borrow_mut().clean_price().unwrap()).abs() <= 1e-11);
        let replacement_model = HullWhite::new(Handle::new(flat(0.05)), 0.1, 0.01).unwrap();
        let replacement = engine(replacement_model, settings.clone());
        bond.set_tree_engine(&replacement).unwrap();
        let replaced = bond.inner.borrow_mut().npv().unwrap();
        quote.set_value(0.07);
        model
            .borrow_mut()
            .set_params(&Array::from([0.15, 0.025]))
            .unwrap();
        assert!(bond.inner.borrow().base().is_calculated());
        assert_eq!(bond.inner.borrow_mut().npv().unwrap(), replaced);
        curve.reset();
        assert!(bond.inner.borrow().base().is_calculated());
        assert_eq!(bond.inner.borrow_mut().npv().unwrap(), replaced);
    }
    #[test]
    fn settings_mismatch_preserves_attachment_and_does_not_refresh_silent_policy() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference());
        let model = HullWhite::new(Handle::new(flat(0.03)), 0.1, 0.01).unwrap();
        let original = engine(model.clone(), settings.clone());
        let mut bond = product(settings.clone());
        bond.set_tree_engine(&original).unwrap();
        let value = bond.inner.borrow_mut().npv().unwrap();
        let notifications = shared(Cell::new(0));
        let observer = shared_mut(Notifications(notifications.clone())) as SharedMut<dyn Observer>;
        bond.inner.borrow().base().register_observer(&observer);
        let wrong_settings = shared(Settings::new());
        wrong_settings.set_evaluation_date(reference());
        let wrong = engine(model, wrong_settings);
        settings.set_include_reference_date_events(true);
        assert!(bond.inner.borrow().bond().base().is_calculated());
        assert!(bond.set_tree_engine(&wrong).is_err());
        let borrowed = bond.inner.borrow();
        assert!(borrowed.bond().base().is_calculated());
        assert_eq!(borrowed.bond().base().results().value, Some(value));
        assert!(SharedMut::ptr_eq(
            borrowed.bond().base().pricing_engine().unwrap(),
            &original.engine()
        ));
        assert_eq!(notifications.get(), 0);
        assert!(!borrowed.base().is_calculated());
        assert_eq!(notifications.get(), 0);
    }
    #[test]
    fn black_live_relink_reset_replacement_and_weak_graph_release() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference());
        let quote = shared(SimpleQuote::new(0.3));
        let weak_quote = Shared::downgrade(&quote);
        let curve = RelinkableHandle::new(flat(0.03));
        let original = PyBlackCallableFixedRateBondEngine {
            inner: shared_mut(BlackCallableFixedRateBondEngine::new(
                Handle::new(quote.clone() as Shared<dyn Quote>),
                curve.handle(),
                settings.clone(),
            )),
            settings: settings.clone(),
        };
        let weak_engine = SharedMut::downgrade(&original.inner);
        let mut bond = product(settings.clone());
        bond.set_black_engine(&original).unwrap();
        let first = bond.inner.borrow_mut().npv().unwrap();
        curve.link_to(flat(0.04));
        assert!(!bond.inner.borrow().bond().base().is_calculated());
        assert_ne!(bond.inner.borrow_mut().npv().unwrap(), first);
        curve.reset();
        assert!(bond.inner.borrow_mut().npv().is_err());
        curve.link_to(flat(0.03));
        assert!((bond.inner.borrow_mut().npv().unwrap() - first).abs() < 1e-8);
        let wrong = PyBlackCallableFixedRateBondEngine {
            inner: shared_mut(BlackCallableFixedRateBondEngine::new(
                Handle::new(quote.clone() as Shared<dyn Quote>),
                curve.handle(),
                shared(Settings::new()),
            )),
            settings: shared(Settings::new()),
        };
        settings.set_include_reference_date_events(true);
        assert!(bond.set_black_engine(&wrong).is_err());
        assert!(bond.inner.borrow().bond().base().is_calculated());
        assert!(SharedMut::ptr_eq(
            bond.inner.borrow().bond().base().pricing_engine().unwrap(),
            &original.engine()
        ));
        let replacement = engine(
            HullWhite::new(Handle::new(flat(0.05)), 0.1, 0.01).unwrap(),
            settings,
        );
        bond.set_tree_engine(&replacement).unwrap();
        let tree_value = bond.inner.borrow_mut().npv().unwrap();
        quote.set_value(0.8);
        curve.reset();
        assert!(bond.inner.borrow().bond().base().is_calculated());
        assert_eq!(bond.inner.borrow_mut().npv().unwrap(), tree_value);
        drop(wrong);
        drop(original);
        drop(quote);
        assert!(weak_engine.upgrade().is_none());
        assert!(weak_quote.upgrade().is_none());
        drop(bond);
        drop(replacement);
    }
}
