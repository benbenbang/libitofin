package itofin

import (
	"errors"
	"math"
	"sync"
	"testing"
)

func hwNear(t *testing.T, got, want, tol float64) {
	t.Helper()
	if math.IsNaN(got) || math.Abs(got-want) > tol {
		t.Fatalf("%.17g != %.17g", got, want)
	}
}
func hwCurve(t *testing.T, s *Session) *YieldTermStructure {
	t.Helper()
	dc := pricingMust(s.Actual365Fixed())
	return pricingMust(s.NewFlatForward(testDate(t, 23, 10, 2026), .03, dc))
}
func hwKernel(a, dt float64) float64 {
	if a == 0 {
		return dt
	}
	return -math.Expm1(-a*dt) / a
}
func TestHullWhiteForwardTransitions(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	dc := pricingMust(s.Actual365Fixed())
	nonflat := pricingMust(s.NewZeroCurve(NodeCurveConfig{Dates: []Date{testDate(t, 23, 10, 2026), testDate(t, 23, 10, 2027), testDate(t, 23, 10, 2029), testDate(t, 23, 10, 2036)}, Values: []float64{.01, .025, .04, .035}, DayCounter: dc}))
	for _, c := range []*YieldTermStructure{hwCurve(t, s), nonflat} {
		for _, a := range []float64{0, 1e-16, 1e-10, .2} {
			p := pricingMust(s.NewHullWhiteForwardProcess(c, a, .02))
			hwNear(t, pricingMust(p.X0()), pricingMust(c.ForwardRate(0, 0, false)), 0)
			if pricingMust(p.A()) != a || pricingMust(p.Sigma()) != .02 || pricingMust(p.ForwardMeasureTime()) != 0 {
				t.Fatal("parameters")
			}
			pricingOK(t, p.SetForwardMeasureTime(5))
			t0, x, dt, dw := 1., .03, .8, -.7
			f := pricingMust(c.ForwardRate(t0, t0, false))
			fp := (pricingMust(c.ForwardRate(t0+.0001, t0+.0001, false)) - f) / .0001
			alpha := f + .5*math.Pow(.02*hwKernel(a, t0), 2)
			hwNear(t, pricingMust(p.Alpha(t0)), alpha, 2e-16)
			hwNear(t, pricingMust(p.B(t0, 5)), hwKernel(a, 4), 1e-15)
			hwNear(t, pricingMust(p.B(5, t0)), hwKernel(a, -4), 1e-15)
			mt := .02 * .02 * hwKernel(a, dt) * (hwKernel(a, 5-t0-dt) + .5*math.Exp(-a*(5-t0-dt))*hwKernel(a, dt))
			hwNear(t, pricingMust(p.MT(t0, t0+dt, 5)), mt, 2e-18)
			drift := -a*x + .02*.02*(.5*hwKernel(a, t0)*(1+math.Exp(-a*t0))-hwKernel(a, 5-t0)) + a*f + fp
			hwNear(t, pricingMust(p.Drift(t0, x)), drift, 2e-16)
			if pricingMust(p.Diffusion(t0, x)) != .02 {
				t.Fatal("diffusion")
			}
			alphaEnd := pricingMust(c.ForwardRate(t0+dt, t0+dt, false)) + .5*math.Pow(.02*hwKernel(a, t0+dt), 2)
			mean := x*math.Exp(-a*dt) + alphaEnd - alpha*math.Exp(-a*dt) - mt
			hwNear(t, pricingMust(p.Expectation(t0, x, dt)), mean, 2e-16)
			variance := .02 * .02 * dt
			if a >= math.Sqrt(math.Nextafter(1, 2)-1) {
				variance = .02 * .02 * hwKernel(2*a, dt)
			}
			hwNear(t, pricingMust(p.Variance(t0, x, dt)), variance, 2e-18)
			hwNear(t, pricingMust(p.StdDeviation(t0, x, dt)), math.Sqrt(variance), 2e-17)
			hwNear(t, pricingMust(p.Evolve(t0, x, dt, dw)), mean+math.Sqrt(variance)*dw, 2e-16)
			if pricingMust(p.Expectation(t0, x, 0)) != x || pricingMust(p.Evolve(t0, x, 0, dw)) != x || pricingMust(p.Variance(t0, x, 0)) != 0 {
				t.Fatal("zero step")
			}
			pricingOK(t, p.Close())
		}
	}
}
func TestHullWhiteForwardRetentionAndHorizon(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	q := pricingMust(s.NewSimpleQuote(.03))
	dc := pricingMust(s.Actual365Fixed())
	c := pricingMust(s.NewFlatForwardFromQuote(testDate(t, 23, 10, 2026), q, dc))
	p := pricingMust(s.NewHullWhiteForwardProcess(c, .1, .02))
	initial, before := pricingMust(p.X0()), pricingMust(p.Alpha(1))
	beforeForward := pricingMust(c.ForwardRate(1, 1, false))
	pricingOK(t, q.SetValue(.05))
	hwNear(t, pricingMust(p.Alpha(1))-before, pricingMust(c.ForwardRate(1, 1, false))-beforeForward, 2e-16)
	mean := pricingMust(p.Expectation(1, .03, .8))
	pricingOK(t, p.SetForwardMeasureTime(5))
	if pricingMust(p.Expectation(1, .03, .8)) == mean {
		t.Fatal("stale horizon")
	}
	fresh := pricingMust(s.NewHullWhiteForwardProcess(c, .1, .02))
	pricingOK(t, fresh.SetForwardMeasureTime(5))
	hwNear(t, pricingMust(p.Expectation(1, .03, .8)), pricingMust(fresh.Expectation(1, .03, .8)), 0)
	for _, bad := range []float64{-1, math.NaN(), math.Inf(1)} {
		if p.SetForwardMeasureTime(bad) == nil || pricingMust(p.ForwardMeasureTime()) != 5 {
			t.Fatal("invalid horizon changed state")
		}
	}
	pricingOK(t, c.Close())
	pricingOK(t, dc.Close())
	pricingOK(t, q.SetValue(.07))
	pricingOK(t, q.Close())
	independentDC := pricingMust(s.Actual365Fixed())
	independentCurve := pricingMust(s.NewFlatForward(testDate(t, 23, 10, 2026), .07, independentDC))
	hwNear(t, pricingMust(p.Alpha(1))-before, pricingMust(independentCurve.ForwardRate(1, 1, false))-beforeForward, 2e-16)
	hwNear(t, pricingMust(p.X0()), initial, 0)
	pricingOK(t, p.SetForwardMeasureTime(0))
	_ = pricingMust(p.Expectation(2, .03, 1))
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 20; j++ {
				if _, err := p.Alpha(1); err != nil {
					t.Error(err)
				}
			}
		}()
	}
	wg.Wait()
}
func TestHullWhiteForwardErrorsAndLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	c := hwCurve(t, s)
	for _, v := range [][2]float64{{-1, .02}, {math.NaN(), .02}, {math.Inf(1), .02}, {.1, -.02}, {.1, math.NaN()}, {.1, math.Inf(1)}} {
		if _, err := s.NewHullWhiteForwardProcess(c, v[0], v[1]); err == nil {
			t.Fatal("invalid parameters")
		}
	}
	if _, err := s.NewHullWhiteForwardProcess(nil, .1, .02); err == nil {
		t.Fatal("nil curve")
	}
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	if _, err := other.NewHullWhiteForwardProcess(c, .1, .02); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	p := pricingMust(s.NewHullWhiteForwardProcess(c, .1, .02))
	calls := []func() (float64, error){func() (float64, error) { return p.Alpha(-1) }, func() (float64, error) { return p.B(0, math.NaN()) }, func() (float64, error) { return p.MT(2, 1, 5) }, func() (float64, error) { return p.Drift(0, math.Inf(1)) }, func() (float64, error) { return p.Diffusion(math.NaN(), 0) }, func() (float64, error) { return p.Expectation(0, 0, -1) }, func() (float64, error) { return p.StdDeviation(0, math.NaN(), 1) }, func() (float64, error) { return p.Variance(1e308, 0, 1e308) }, func() (float64, error) { return p.Evolve(0, 0, 1, math.NaN()) }}
	for _, call := range calls {
		if _, err := call(); err == nil {
			t.Fatal("invalid query")
		}
	}
	zero := pricingMust(s.NewHullWhiteForwardProcess(c, 0, 0))
	if pricingMust(zero.Variance(0, .03, 5)) != 0 || pricingMust(zero.Diffusion(0, .03)) != 0 || pricingMust(zero.MT(1, 2, 5)) != 0 {
		t.Fatal("zero vol")
	}
	var nilP *HullWhiteForwardProcess
	if _, err := nilP.X0(); err == nil {
		t.Fatal("nil process")
	}
	if nilP.SetForwardMeasureTime(1) == nil {
		t.Fatal("nil setter")
	}
	var empty HullWhiteForwardProcess
	if _, err := empty.X0(); err == nil {
		t.Fatal("empty process")
	}
	if empty.SetForwardMeasureTime(1) == nil {
		t.Fatal("empty setter")
	}
	pricingOK(t, p.Close())
	pricingOK(t, p.Close())
	if _, err := p.X0(); err == nil {
		t.Fatal("released process")
	}
	pricingOK(t, c.Close())
	if _, err := s.NewHullWhiteForwardProcess(c, .1, .02); err == nil {
		t.Fatal("released curve")
	}
	pricingOK(t, s.Close())
	if _, err := zero.X0(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}
