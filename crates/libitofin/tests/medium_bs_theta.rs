use libitofin::errors::QlResult;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::matrix::Matrix;
use libitofin::patterns::observable::{AsObservable, Observable};
use libitofin::pricingengines::{black_scholes_theta, default_theta_per_day};
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    BlackConstantVol, BlackVarianceCurve, BlackVarianceSurface, BlackVolTermStructure,
    LocalConstantVol, LocalVolTermStructure, VolatilityTermStructure,
};
use libitofin::termstructures::yields::{FlatForward, ZeroCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::termstructures::{TermStructure, TermStructureBase};
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn reference() -> Date {
    Date::new(9, Month::October, 2026)
}

fn flat(quote: Shared<SimpleQuote>) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::new(
        reference(),
        Handle::new(quote as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

fn black(quote: Shared<SimpleQuote>) -> Shared<dyn BlackVolTermStructure> {
    shared(BlackConstantVol::with_quote(
        reference(),
        None,
        Handle::new(quote as Shared<dyn Quote>),
        Actual365Fixed::new(),
    ))
}

struct Market {
    spot: Shared<SimpleQuote>,
    r: Shared<SimpleQuote>,
    q: Shared<SimpleQuote>,
    vol: Shared<SimpleQuote>,
    risk: RelinkableHandle<dyn YieldTermStructure>,
    dividend: RelinkableHandle<dyn YieldTermStructure>,
    black: RelinkableHandle<dyn BlackVolTermStructure>,
    process: GeneralizedBlackScholesProcess,
}

impl Market {
    fn new() -> Self {
        let spot = shared(SimpleQuote::new(100.0));
        let r = shared(SimpleQuote::new(0.05));
        let q = shared(SimpleQuote::new(0.02));
        let vol = shared(SimpleQuote::new(0.20));
        let risk = RelinkableHandle::new(flat(r.clone()));
        let dividend = RelinkableHandle::new(flat(q.clone()));
        let black = RelinkableHandle::new(black(vol.clone()));
        let process = GeneralizedBlackScholesProcess::new(
            Handle::new(spot.clone() as Shared<dyn Quote>),
            dividend.handle(),
            risk.handle(),
            black.handle(),
        );
        Self {
            spot,
            r,
            q,
            vol,
            risk,
            dividend,
            black,
            process,
        }
    }

    fn theta(&self) -> QlResult<f64> {
        black_scholes_theta(&self.process, 12.0, 0.6, 0.02)
    }
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 3e-10, "{actual} != {expected}");
}

#[test]
fn compiled_quantlib_analytic_vanilla_theta_matches_source_identity() {
    let market = Market::new();
    for (value, delta, gamma, theta) in [
        (
            9.227005508154061,
            0.5868511461347649,
            0.01895057875500871,
            -5.089318913998339,
        ),
        (
            10.8618095981619,
            -0.6840206078245822,
            0.024702095378360527,
            -2.345266772290288,
        ),
        (
            5.136174307078467,
            0.31864001943984643,
            0.01232628276740547,
            -3.164367896446711,
        ),
        (
            0.5220194179344878,
            -0.16668605960202326,
            0.04355279802475387,
            -8.184400455247946,
        ),
    ] {
        close(
            black_scholes_theta(&market.process, value, delta, gamma).unwrap(),
            theta,
        );
    }
}

#[test]
fn current_quotes_and_relinked_nonflat_curves_match_quantlib() {
    let market = Market::new();
    close(market.theta().unwrap(), -5.19999999999906);
    market.spot.set_value(115.0);
    market.r.set_value(-0.01);
    market.q.set_value(0.03);
    market.vol.set_value(0.35);
    close(market.theta().unwrap(), -13.560625000059611);
    let dates = vec![reference(), reference() + 182, reference() + 365];
    market.risk.link_to(shared(
        ZeroCurve::new(
            dates.clone(),
            vec![0.03, 0.07, 0.09],
            Actual365Fixed::new(),
            Linear,
        )
        .unwrap(),
    ));
    market.dividend.link_to(shared(
        ZeroCurve::new(
            dates,
            vec![0.01, 0.025, 0.04],
            Actual365Fixed::new(),
            Linear,
        )
        .unwrap(),
    ));
    close(market.theta().unwrap(), -17.220874684105866);
    let local = RelinkableHandle::<dyn LocalVolTermStructure>::new(shared(LocalConstantVol::new(
        reference(),
        0.12,
        Actual365Fixed::new(),
    )));
    let process = GeneralizedBlackScholesProcess::with_local_vol(
        market.process.state_variable(),
        market.dividend.handle(),
        market.risk.handle(),
        market.black.handle(),
        local.handle(),
    );
    close(
        black_scholes_theta(&process, 12.0, 0.6, 0.02).unwrap(),
        -2.9246496841058685,
    );
    local.link_to(shared(LocalConstantVol::new(
        reference(),
        0.0,
        Actual365Fixed::new(),
    )));
    close(
        black_scholes_theta(&process, 12.0, 0.6, 0.02).unwrap(),
        -1.0202496841058681,
    );
}

#[test]
fn spot_and_black_curve_relink_refresh_derived_local_volatility() {
    let market = Market::new();
    let spot = RelinkableHandle::<dyn Quote>::new(market.spot.clone());
    let process = GeneralizedBlackScholesProcess::new(
        spot.handle(),
        market.dividend.handle(),
        market.risk.handle(),
        market.black.handle(),
    );
    close(
        black_scholes_theta(&process, 12.0, 0.6, 0.02).unwrap(),
        -5.2,
    );
    spot.link_to(shared(SimpleQuote::new(120.0)));
    market.black.link_to(black(shared(SimpleQuote::new(0.3))));
    close(
        black_scholes_theta(&process, 12.0, 0.6, 0.02).unwrap(),
        -14.52,
    );
}

struct SpotLocalVol {
    base: TermStructureBase,
}

impl AsObservable for SpotLocalVol {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for SpotLocalVol {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl VolatilityTermStructure for SpotLocalVol {
    fn business_day_convention(&self) -> BusinessDayConvention {
        BusinessDayConvention::Following
    }
    fn min_strike(&self) -> f64 {
        90.0
    }
    fn max_strike(&self) -> f64 {
        150.0
    }
}

impl LocalVolTermStructure for SpotLocalVol {
    fn local_vol_impl(&self, t: f64, spot: f64) -> QlResult<f64> {
        assert_eq!(t, 0.0);
        Ok(spot / 1000.0)
    }
}

#[test]
fn helper_queries_local_vol_at_current_spot_and_honors_its_domain() {
    let market = Market::new();
    let local = shared(SpotLocalVol {
        base: TermStructureBase::with_reference_date(
            reference(),
            None,
            Some(Actual365Fixed::new()),
        ),
    });
    let process = GeneralizedBlackScholesProcess::with_local_vol(
        market.process.state_variable(),
        market.dividend.handle(),
        market.risk.handle(),
        market.black.handle(),
        Handle::new(local as Shared<dyn LocalVolTermStructure>),
    );
    close(
        black_scholes_theta(&process, 12.0, 0.6, 0.02).unwrap(),
        -2.2,
    );
    market.spot.set_value(120.0);
    close(
        black_scholes_theta(&process, 12.0, 0.6, 0.02).unwrap(),
        -3.6336,
    );
    market.spot.set_value(151.0);
    assert!(black_scholes_theta(&process, 12.0, 0.6, 0.02).is_err());
}

#[test]
fn signed_greeks_negative_rates_and_zero_volatility_are_supported() {
    let market = Market::new();
    market.r.set_value(-0.01);
    market.q.set_value(-0.02);
    market.vol.set_value(0.0);
    close(
        black_scholes_theta(&market.process, -12.0, -0.6, -0.02).unwrap(),
        0.72,
    );
}

#[test]
fn nonfinite_arguments_invalid_markets_and_overflow_return_errors() {
    let market = Market::new();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (value, delta, gamma) in [
            (invalid, 0.6, 0.02),
            (12.0, invalid, 0.02),
            (12.0, 0.6, invalid),
        ] {
            assert!(black_scholes_theta(&market.process, value, delta, gamma).is_err());
        }
    }
    for spot in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        market.spot.set_value(spot);
        assert!(market.theta().is_err());
    }
    market.spot.reset();
    assert!(market.theta().is_err());
    market.spot.set_value(100.0);
    for quote in [&market.r, &market.q, &market.vol] {
        let original = quote.value().unwrap();
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            quote.set_value(invalid);
            assert!(market.theta().is_err());
        }
        quote.reset();
        assert!(market.theta().is_err());
        quote.set_value(original);
    }
    market.vol.set_value(-0.2);
    assert!(market.theta().is_err());
    market.vol.set_value(f64::MAX);
    assert!(market.theta().is_err());
    market.vol.set_value(0.2);
    assert!(black_scholes_theta(&market.process, 12.0, 0.6, f64::MAX).is_err());
    market.spot.set_value(f64::MAX);
    assert!(market.theta().is_err());
    assert!(default_theta_per_day(f64::NAN).is_nan());
    assert_eq!(default_theta_per_day(f64::INFINITY), f64::INFINITY);
}

#[test]
fn empty_handles_propagate_errors() {
    let market = Market::new();
    let process = GeneralizedBlackScholesProcess::new(
        Handle::empty(),
        market.dividend.handle(),
        market.risk.handle(),
        market.black.handle(),
    );
    assert!(black_scholes_theta(&process, 12.0, 0.6, 0.02).is_err());
    let process = GeneralizedBlackScholesProcess::new(
        market.process.state_variable(),
        Handle::empty(),
        market.risk.handle(),
        market.black.handle(),
    );
    assert!(black_scholes_theta(&process, 12.0, 0.6, 0.02).is_err());
    let process = GeneralizedBlackScholesProcess::new(
        market.process.state_variable(),
        market.dividend.handle(),
        Handle::empty(),
        market.black.handle(),
    );
    assert!(black_scholes_theta(&process, 12.0, 0.6, 0.02).is_err());
    let process = GeneralizedBlackScholesProcess::new(
        market.process.state_variable(),
        market.dividend.handle(),
        market.risk.handle(),
        Handle::empty(),
    );
    assert!(black_scholes_theta(&process, 12.0, 0.6, 0.02).is_err());
    let process = GeneralizedBlackScholesProcess::with_local_vol(
        market.process.state_variable(),
        market.dividend.handle(),
        market.risk.handle(),
        market.black.handle(),
        Handle::empty(),
    );
    assert!(black_scholes_theta(&process, 12.0, 0.6, 0.02).is_err());
}

#[test]
fn unsupported_derived_surface_errors_but_external_local_vol_is_sufficient() {
    let market = Market::new();
    let surface = BlackVarianceSurface::new(
        reference(),
        None,
        &[reference() + 182, reference() + 365],
        vec![90.0, 110.0],
        &Matrix::filled(2, 2, 0.2),
        Actual365Fixed::new(),
    )
    .unwrap();
    market.black.link_to(shared(surface));
    assert!(market.theta().is_err());
    let process = GeneralizedBlackScholesProcess::with_local_vol(
        market.process.state_variable(),
        market.dividend.handle(),
        market.risk.handle(),
        market.black.handle(),
        Handle::new(shared(LocalConstantVol::new(
            reference(),
            0.2,
            Actual365Fixed::new(),
        )) as Shared<dyn LocalVolTermStructure>),
    );
    close(
        black_scholes_theta(&process, 12.0, 0.6, 0.02).unwrap(),
        -5.2,
    );
}

#[test]
fn linear_black_variance_curve_supplies_time_zero_local_variance_slope() {
    let market = Market::new();
    market.black.link_to(shared(
        BlackVarianceCurve::new(
            reference(),
            &[reference() + 182, reference() + 365],
            &[0.18, 0.26],
            Actual365Fixed::new(),
            true,
        )
        .unwrap(),
    ));
    close(market.theta().unwrap(), -4.439999999999061);
}
