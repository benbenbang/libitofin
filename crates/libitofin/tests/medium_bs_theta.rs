use libitofin::errors::QlResult;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::pricingengines::black_scholes_theta;
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    BlackConstantVol, BlackVolTermStructure, LocalConstantVol, LocalVolTermStructure,
};
use libitofin::termstructures::yields::{FlatForward, ZeroCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
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
