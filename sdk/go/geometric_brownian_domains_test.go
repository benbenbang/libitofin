package itofin

import (
	"math"
	"testing"
)

func TestGeometricBrownianConstructorDomains(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, params := range [][3]float64{
		{math.NaN(), .1, .2}, {math.Inf(1), .1, .2},
		{100, math.NaN(), .2}, {100, math.Inf(-1), .2},
		{100, .1, -.2}, {100, .1, math.Inf(1)}, {100, .1, math.NaN()},
	} {
		if p, err := s.NewGeometricBrownianMotionProcess(params[0], params[1], params[2]); err == nil || p != nil {
			t.Fatalf("invalid constructor %v: %v, %v", params, p, err)
		}
	}
	p := pricingMust(s.NewGeometricBrownianMotionProcess(0, 0, 0))
	pricingNear(t, pricingMust(p.Evolve(0, 0, 1, -7)), 0, 0)
	q := pricingMust(s.NewGeometricBrownianMotionProcess(-1, -.1, 0))
	pricingNear(t, pricingMust(q.X0()), -1, 0)
}

func TestGeometricBrownianTransitionDomains(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewGeometricBrownianMotionProcess(100, .08, .2))
	for _, bad := range []float64{math.NaN(), math.Inf(1), -1} {
		for _, call := range []func() (float64, error){
			func() (float64, error) { return p.Drift(bad, 100) },
			func() (float64, error) { return p.Diffusion(bad, 100) },
			func() (float64, error) { return p.Expectation(bad, 100, .25) },
			func() (float64, error) { return p.Variance(bad, 100, .25) },
			func() (float64, error) { return p.StdDeviation(bad, 100, .25) },
			func() (float64, error) { return p.Evolve(bad, 100, .25, 1) },
			func() (float64, error) { return p.Expectation(0, 100, bad) },
			func() (float64, error) { return p.Variance(0, 100, bad) },
			func() (float64, error) { return p.StdDeviation(0, 100, bad) },
			func() (float64, error) { return p.Evolve(0, 100, bad, 1) },
		} {
			if _, err := call(); err == nil {
				t.Fatalf("invalid time/step %g accepted", bad)
			}
		}
	}
	for _, bad := range []float64{math.NaN(), math.Inf(-1)} {
		for _, call := range []func() (float64, error){
			func() (float64, error) { return p.Drift(0, bad) },
			func() (float64, error) { return p.Diffusion(0, bad) },
			func() (float64, error) { return p.Expectation(0, bad, .25) },
			func() (float64, error) { return p.Variance(0, bad, .25) },
			func() (float64, error) { return p.StdDeviation(0, bad, .25) },
			func() (float64, error) { return p.Evolve(0, bad, .25, 1) },
			func() (float64, error) { return p.Evolve(0, 100, .25, bad) },
		} {
			if _, err := call(); err == nil {
				t.Fatalf("nonfinite state/draw %g accepted", bad)
			}
		}
	}
	pricingNear(t, pricingMust(p.Evolve(.5, 100, .25, .75)), 109.5, 0)
}

func TestGeometricBrownianOverflowAndZeroStep(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewGeometricBrownianMotionProcess(math.MaxFloat64, 2, 2))
	for _, call := range []func() (float64, error){
		func() (float64, error) { return p.Drift(0, math.MaxFloat64) },
		func() (float64, error) { return p.Diffusion(0, math.MaxFloat64) },
		func() (float64, error) { return p.Expectation(0, math.MaxFloat64, 1) },
		func() (float64, error) { return p.Variance(0, math.MaxFloat64, 1) },
		func() (float64, error) { return p.StdDeviation(0, math.MaxFloat64, 1) },
		func() (float64, error) { return p.Evolve(0, math.MaxFloat64, 1, 1) },
	} {
		if _, err := call(); err == nil {
			t.Fatal("overflow accepted")
		}
	}
	pricingNear(t, pricingMust(p.Expectation(0, math.MaxFloat64, 0)), math.MaxFloat64, 0)
	pricingNear(t, pricingMust(p.Evolve(0, math.MaxFloat64, 0, 1)), math.MaxFloat64, 0)
	pricingNear(t, pricingMust(p.Variance(0, math.MaxFloat64, 0)), 0, 0)
	pricingNear(t, pricingMust(p.StdDeviation(0, math.MaxFloat64, 0)), 0, 0)
}
