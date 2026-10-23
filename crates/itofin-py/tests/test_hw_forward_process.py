"""Standalone Hull-White forward process, independent flat-curve laws and retention."""
import gc
import math

import pytest
from itofin import ItofinError
from itofin.processes import HullWhiteForwardProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, ZeroCurve
from itofin.time import Date, DayCounter


def curve(rate=0.03):
    return FlatForward(Date(23, 10, 2026), rate, DayCounter.actual365_fixed())


def kernel(a, dt):
    return dt if a == 0 else -math.expm1(-a * dt) / a


@pytest.mark.parametrize("a", [0., 1e-16, 1e-10, .2])
@pytest.mark.parametrize("nonflat", [False, True])
def test_hw_forward_transitions_and_independent_limits(a, nonflat):
    c = ZeroCurve([Date(23,10,2026),Date(23,10,2027),Date(23,10,2029),Date(23,10,2036)],
                  [.01,.025,.04,.035],DayCounter.actual365_fixed()) if nonflat else curve()
    p = HullWhiteForwardProcess(c, a, .02)
    x0 = p.x0()
    assert x0 == c.forward_rate(0., 0., False)
    assert p.a() == a
    assert p.sigma() == .02
    assert p.forward_measure_time() == 0.
    p.set_forward_measure_time(5.)
    t, x, dt, dw = 1., .03, .8, -.7
    alpha = c.forward_rate(t, t, False) + .5 * (.02 * kernel(a, t))**2
    assert p.alpha(t) == pytest.approx(alpha, rel=0, abs=2e-16)
    assert p.b(t, 5.) == pytest.approx(kernel(a, 4.), rel=0, abs=1e-15)
    assert p.b(5., t) == pytest.approx(kernel(a, -4.), rel=0, abs=1e-15)
    mt = .02**2 * kernel(a, dt) * (kernel(a, 5.-t-dt) + .5*math.exp(-a*(5.-t-dt))*kernel(a, dt))
    assert p.m_t(t, t+dt, 5.) == pytest.approx(mt, rel=0, abs=2e-18)
    f = c.forward_rate(t, t, False)
    fp = (c.forward_rate(t+.0001, t+.0001, False)-f)/.0001
    drift = -a*x+.02**2*(.5*kernel(a,t)*(1+math.exp(-a*t))-kernel(a,5.-t))+a*f+fp
    assert p.drift(t,x) == pytest.approx(drift, rel=0, abs=2e-16)
    assert p.diffusion(t,x) == .02
    alpha_end = c.forward_rate(t+dt,t+dt,False)+.5*(.02*kernel(a,t+dt))**2
    mean = x*math.exp(-a*dt)+alpha_end-alpha*math.exp(-a*dt)-mt
    assert p.expectation(t,x,dt) == pytest.approx(mean, rel=0, abs=2e-16)
    variance = .02**2*(dt if a < math.sqrt(2.220446049250313e-16) else kernel(2*a,dt))
    assert p.variance(t,x,dt) == pytest.approx(variance, rel=0, abs=2e-18)
    assert p.std_deviation(t,x,dt) == pytest.approx(math.sqrt(variance), rel=0, abs=2e-17)
    assert p.evolve(t,x,dt,dw) == pytest.approx(mean+math.sqrt(variance)*dw, rel=0, abs=2e-16)
    assert p.expectation(t,x,0.) == x
    assert p.evolve(t,x,0.,dw) == x
    assert p.variance(t,x,0.) == 0.


def test_hw_forward_horizon_live_curve_and_retained_lifetimes():
    q = SimpleQuote(.03)
    c = FlatForward.from_quote(Date(23,10,2026),q,DayCounter.actual365_fixed())
    p = HullWhiteForwardProcess(c,.1,.02)
    initial, before = p.x0(), p.alpha(1.)
    before_forward = c.forward_rate(1.,1.,False)
    q.set_value(.05)
    assert p.alpha(1.)-before == pytest.approx(c.forward_rate(1.,1.,False)-before_forward, rel=0, abs=2e-16)
    assert p.x0() == initial
    mean = p.expectation(1.,.03,.8)
    p.set_forward_measure_time(5.)
    assert p.expectation(1.,.03,.8) != mean
    fresh = HullWhiteForwardProcess(c,.1,.02)
    fresh.set_forward_measure_time(5.)
    assert p.expectation(1.,.03,.8) == fresh.expectation(1.,.03,.8)
    for bad in [-1.,math.nan,math.inf]:
        with pytest.raises(ItofinError):
            p.set_forward_measure_time(bad)
        assert p.forward_measure_time() == 5.
    del c, fresh
    gc.collect()
    q.set_value(.07)
    assert p.alpha(1.)-before == pytest.approx(curve(.07).forward_rate(1.,1.,False)-before_forward, rel=0, abs=2e-16)
    del q
    gc.collect()
    assert math.isfinite(p.drift(1.,.03))
    assert p.x0() == initial
    p.set_forward_measure_time(0.)
    assert math.isfinite(p.expectation(2.,.03,1.))


@pytest.mark.parametrize("a,sigma", [(-1.,.02),(math.nan,.02),(math.inf,.02),(.1,-.02),(.1,math.nan),(.1,math.inf)])
def test_hw_forward_invalid_parameters(a,sigma):
    with pytest.raises(ItofinError):
        HullWhiteForwardProcess(curve(),a,sigma)


def test_hw_forward_invalid_queries_zero_vol_and_curve_bounds():
    p = HullWhiteForwardProcess(curve(),.1,.02)
    calls = [lambda:p.alpha(-1.),lambda:p.b(0.,math.nan),lambda:p.m_t(2.,1.,5.),
             lambda:p.drift(0.,math.inf),lambda:p.diffusion(math.nan,0.),
             lambda:p.expectation(0.,0.,-1.),lambda:p.std_deviation(0.,math.nan,1.),
             lambda:p.variance(1e308,0.,1e308),lambda:p.evolve(0.,0.,1.,math.nan),
             lambda:p.evolve(0.,.03,0.,math.nan),lambda:p.variance(0.,math.nan,0.)]
    for call in calls:
        with pytest.raises(ItofinError):
            call()
    zero = HullWhiteForwardProcess(curve(),0.,0.)
    assert zero.variance(0.,.03,5.) == zero.diffusion(0.,.03) == zero.m_t(1.,2.,5.) == 0.
    assert zero.expectation(0.,.03,1.) == pytest.approx(.03, rel=0, abs=2e-12)
    with pytest.raises(TypeError):
        getattr(HullWhiteForwardProcess, "__call__")(None,.1,.02)
    bounded = ZeroCurve([Date(23,10,2026),Date(23,10,2027)],[.03,.04],DayCounter.actual365_fixed())
    p = HullWhiteForwardProcess(bounded,.1,.02)
    with pytest.raises(ItofinError):
        p.alpha(2.)
