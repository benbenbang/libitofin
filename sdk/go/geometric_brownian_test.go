package itofin

import (
	"errors"
	"math"
	"sync"
	"testing"
)

func TestGeometricBrownianEulerTransitions(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewGeometricBrownianMotionProcess(100, .08, .2))
	values := []float64{
		pricingMust(p.X0()), pricingMust(p.Mu()), pricingMust(p.Volatility()),
		pricingMust(p.Drift(.5, 100)), pricingMust(p.Diffusion(.5, 100)),
		pricingMust(p.Expectation(.5, 100, .25)), pricingMust(p.Variance(.5, 100, .25)),
		pricingMust(p.StdDeviation(.5, 100, .25)), pricingMust(p.Evolve(.5, 100, .25, .75)),
	}
	want := []float64{100, .08, .2, 8, 20, 102, 100, 10, 109.5}
	for i, v := range values {
		if math.Float64bits(v) != math.Float64bits(want[i]) {
			t.Fatalf("query %d: got %.17g want %.17g", i, v, want[i])
		}
	}
	if pricingMust(p.Expectation(.5, 100, .25)) == 100*math.Exp(.08*.25) {
		t.Fatal("unexpected exact exponential mean")
	}
	pricingNear(t, pricingMust(p.StdDeviation(.5, -100, .25)), -10, 0)
	pricingNear(t, pricingMust(p.Evolve(0, 100, 1, -10)), -92, 0)
	negativeZero := math.Copysign(0, -1)
	if math.Float64bits(pricingMust(p.Evolve(0, negativeZero, 0, 7))) != math.Float64bits(negativeZero) {
		t.Fatal("zero step did not preserve negative zero")
	}
	q := pricingMust(s.NewGeometricBrownianMotionProcess(-10, -.4, 0))
	pricingNear(t, pricingMust(q.Evolve(0, -10, .5, 50)), -8, 0)
	pricingNear(t, pricingMust(q.Drift(0, -10)), 4, 0)
	pricingNear(t, pricingMust(q.Variance(0, -10, .5)), 0, 0)
}

func TestGeometricBrownianLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewGeometricBrownianMotionProcess(100, .08, .2))
	copy := *p
	pricingOK(t, p.Close())
	pricingOK(t, copy.Close())
	if _, err := copy.X0(); err == nil {
		t.Fatal("stale copied wrapper accepted")
	}
	q := pricingMust(s.NewGeometricBrownianMotionProcess(50, .1, .3))
	pricingNear(t, pricingMust(q.X0()), 50, 0)
	if q.id == p.id {
		t.Fatal("released handle reused")
	}
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	foreign := &GeometricBrownianMotionProcess{object{other, q.id}}
	if _, err := foreign.X0(); err == nil {
		t.Fatal("foreign session native handle accepted")
	}
	quote := pricingMust(s.NewSimpleQuote(50))
	wrong := &GeometricBrownianMotionProcess{quote.object}
	if _, err := wrong.X0(); err == nil {
		t.Fatal("wrong handle type accepted")
	}
	pricingNear(t, pricingMust(q.X0()), 50, 0)
	pricingOK(t, s.Close())
	if _, err := q.X0(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
	if _, err := s.NewGeometricBrownianMotionProcess(100, .1, .2); !errors.Is(err, ErrClosed) {
		t.Fatalf("constructor after session close: %v", err)
	}
	pricingOK(t, q.Close())
	pricingOK(t, s.Close())
}

func TestGeometricBrownianNilAndZeroWrappers(t *testing.T) {
	var s *Session
	if _, err := s.NewGeometricBrownianMotionProcess(100, .1, .2); err == nil {
		t.Fatal("nil session accepted")
	}
	for _, p := range []*GeometricBrownianMotionProcess{nil, {}} {
		for _, call := range []func() (float64, error){
			p.X0, p.Mu, p.Volatility,
			func() (float64, error) { return p.Drift(0, 100) },
			func() (float64, error) { return p.Diffusion(0, 100) },
			func() (float64, error) { return p.Expectation(0, 100, 1) },
			func() (float64, error) { return p.Variance(0, 100, 1) },
			func() (float64, error) { return p.StdDeviation(0, 100, 1) },
			func() (float64, error) { return p.Evolve(0, 100, 1, 0) },
		} {
			if _, err := call(); err == nil {
				t.Fatal("nil/zero process accepted")
			}
		}
	}
}

func TestGeometricBrownianConcurrentSessionCalls(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewGeometricBrownianMotionProcess(100, .08, .2))
	var wg sync.WaitGroup
	for range 12 {
		wg.Go(func() {
			for range 10 {
				got, err := p.Evolve(.5, 100, .25, .75)
				if err != nil || got != 109.5 {
					t.Errorf("serialized transition: %g, %v", got, err)
				}
			}
		})
	}
	wg.Wait()
}
