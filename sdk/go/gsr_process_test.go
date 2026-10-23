package itofin

import (
	"errors"
	"math"
	"sync"
	"testing"
)

func gsrNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.Abs(got-want) > 2e-14*math.Max(math.Abs(want), 1e-300) {
		t.Fatalf("%.17g != %.17g", got, want)
	}
}

func TestGsrConstantGridAndTransitions(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, a := range []float64{0, 1e-15, .2, -.2} {
		vols := []float64{.03, .03, .03}
		p := pricingMust(s.NewGsrProcess([]float64{1, 2}, vols, []float64{a, a, a}, 10))
		vols[0] = 9
		if pricingMust(p.X0()) != 0 || pricingMust(p.ForwardMeasureTime()) != 10 || pricingMust(p.Sigma(10)) != .03 || pricingMust(p.Reversion(0)) != a {
			t.Fatal("coefficients")
		}
		kernel := func(k, dt float64) float64 {
			if k == 0 {
				return dt
			}
			return -math.Expm1(-k*dt) / k
		}
		gsrNear(t, pricingMust(p.G(2, 2.8)), kernel(a, .8))
		y := pricingMust(p.Y(2))
		gsrNear(t, y, .03*.03*kernel(2*a, 2))
		gsrNear(t, pricingMust(p.Drift(2, .1)), y-.03*.03*pricingMust(p.G(2, 10))-a*.1)
		if pricingMust(p.Diffusion(2, .1)) != .03 {
			t.Fatal("diffusion")
		}
		variance := .03 * .03 * kernel(2*a, .8)
		gsrNear(t, pricingMust(p.Variance(2, .1, .8)), variance)
		gsrNear(t, pricingMust(p.StdDeviation(2, .1, .8)), math.Sqrt(variance))
		mean := pricingMust(p.Expectation(2, .1, .8))
		gsrNear(t, pricingMust(p.Evolve(2, .1, .8, -.7)), mean-math.Sqrt(variance)*.7)
		if a == 0 {
			gsrNear(t, mean, .1+.03*.03*(2*.8-.8*(10-2-.8)))
		}
		if pricingMust(p.Evolve(2, .1, 0, 4)) != .1 {
			t.Fatal("zero step")
		}
		pricingOK(t, p.Close())
	}
}

func TestGsrHorizonAndLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewGsrProcess([]float64{2}, []float64{.03, .03}, []float64{0}, 10))
	old := pricingMust(p.Expectation(0, .1, 1))
	pricingOK(t, p.SetForwardMeasureTime(5))
	fresh := pricingMust(s.NewGsrProcess(nil, []float64{.03}, []float64{0}, 5))
	if pricingMust(p.Expectation(0, .1, 1)) != pricingMust(fresh.Expectation(0, .1, 1)) {
		t.Fatal("horizon differs from fresh process")
	}
	if pricingMust(p.Expectation(0, .1, 1)) == old {
		t.Fatal("stale horizon")
	}
	for _, bad := range []float64{-1, math.NaN(), math.Inf(1)} {
		if p.SetForwardMeasureTime(bad) == nil || pricingMust(p.ForwardMeasureTime()) != 5 {
			t.Fatal("invalid horizon changed state")
		}
	}
	pricingOK(t, p.SetForwardMeasureTime(0))
	if pricingMust(p.G(0, 0)) != 0 || pricingMust(p.Evolve(0, .3, 0, 2)) != .3 {
		t.Fatal("zero horizon")
	}
	pricingOK(t, p.SetForwardMeasureTime(10))
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 20; j++ {
				if _, err := p.Y(2); err != nil {
					t.Error(err)
				}
			}
		}()
	}
	wg.Wait()
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	foreign := &GsrProcess{object{other, p.id}}
	if _, err := foreign.X0(); err == nil {
		t.Fatal("foreign handle accepted")
	}
	pricingOK(t, p.Close())
	if _, err := p.X0(); err == nil {
		t.Fatal("released handle accepted")
	}
	var nilProcess *GsrProcess
	if _, err := nilProcess.X0(); err == nil {
		t.Fatal("nil accepted")
	}
	if nilProcess.SetForwardMeasureTime(1) == nil {
		t.Fatal("nil setter accepted")
	}
	var zero GsrProcess
	if _, err := zero.X0(); err == nil {
		t.Fatal("zero object accepted")
	}
	q := pricingMust(s.NewGsrProcess(nil, []float64{0}, []float64{-.2}, 10))
	gsrNear(t, pricingMust(q.Expectation(0, .3, 2)), .3*math.Exp(.4))
	if pricingMust(q.Variance(0, .3, 2)) != 0 {
		t.Fatal("zero volatility")
	}
	pricingOK(t, s.Close())
	if _, err := q.X0(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}

func TestGsrRejectsInvalidInputs(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	grids := []struct {
		times, vols, reversions []float64
		horizon                 float64
	}{
		{nil, nil, nil, 10}, {[]float64{1}, []float64{.03}, []float64{.1}, 10},
		{[]float64{1}, []float64{.03, .04}, []float64{.1}, 10},
		{[]float64{1}, []float64{.03, .03}, []float64{.1, .2}, 10},
		{[]float64{1, 1}, []float64{.03, .03, .03}, []float64{.1}, 10},
		{[]float64{-1}, []float64{.03, .03}, []float64{.1}, 10},
		{nil, []float64{-.03}, []float64{.1}, 10},
		{nil, []float64{.03}, []float64{math.NaN()}, 10},
		{nil, []float64{.03}, []float64{.1}, -1},
	}
	for _, g := range grids {
		if _, err := s.NewGsrProcess(g.times, g.vols, g.reversions, g.horizon); err == nil {
			t.Fatal("invalid grid accepted")
		}
	}
	p := pricingMust(s.NewGsrProcess(nil, []float64{.03}, []float64{.1}, 2))
	checks := []func() (float64, error){func() (float64, error) { return p.Sigma(2.1) }, func() (float64, error) { return p.Reversion(-1) }, func() (float64, error) { return p.Y(math.NaN()) }, func() (float64, error) { return p.G(1, .5) }, func() (float64, error) { return p.Drift(0, math.Inf(1)) }, func() (float64, error) { return p.Diffusion(-1, 0) }, func() (float64, error) { return p.Expectation(1.5, 0, .6) }, func() (float64, error) { return p.StdDeviation(0, math.NaN(), 1) }, func() (float64, error) { return p.Variance(0, 0, -1) }, func() (float64, error) { return p.Evolve(0, 0, 1, math.NaN()) }}
	for _, check := range checks {
		if _, err := check(); err == nil {
			t.Fatal("invalid query accepted")
		}
	}
}
