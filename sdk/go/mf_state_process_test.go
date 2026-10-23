package itofin

import (
	"math"
	"testing"
)

func mfNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.IsNaN(want) || math.IsInf(want, 0) || math.Abs(got-want) > 2e-13*math.Max(math.Abs(want), math.SmallestNonzeroFloat64*(1<<52)) {
		t.Fatalf("%.17g != %.17g", got, want)
	}
}

func TestMfStateProcess(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	times, vols := []float64{0, 1, 2}, []float64{.9, .2, .3, .4}
	p := pricingMust(s.NewMfStateProcess(.7, times, vols))
	times[1], vols[1] = 9, 9
	if pricingMust(p.Reversion()) != .7 || pricingMust(p.X0()) != 0 || pricingMust(p.Drift(2, 7)) != 0 {
		t.Fatal("parameters")
	}
	for _, pair := range [][2]float64{{0, .2}, {.999, .2}, {1, .3}, {2, .4}, {10, .4}} {
		if pricingMust(p.Diffusion(pair[0], -7)) != pair[1] {
			t.Fatal("right-continuous raw diffusion")
		}
	}
	copiedTimes, copiedVols := pricingMust(p.Times()), pricingMust(p.Volatilities())
	copiedTimes[0], copiedVols[1] = 9, 9
	if pricingMust(p.Times())[0] != 0 || pricingMust(p.Volatilities())[1] != .2 {
		t.Fatal("grid getters alias process")
	}
	if pricingMust(p.Expectation(.5, -.2, 2)) != -.2 {
		t.Fatal("mean")
	}
	v := pricingMust(p.Variance(.5, -.2, 2))
	mfNear(t, pricingMust(p.StdDeviation(.5, -.2, 2)), math.Sqrt(v))
	mfNear(t, pricingMust(p.Evolve(.5, -.2, 2, -.7)), -.2-math.Sqrt(v)*.7)
	if pricingMust(p.Evolve(0, .3, 0, 5)) != .3 {
		t.Fatal("zero step")
	}
	for _, a := range []float64{-.4, 0, .4} {
		empty := pricingMust(s.NewMfStateProcess(a, nil, []float64{0}))
		if len(pricingMust(empty.Times())) != 0 || pricingMust(empty.Diffusion(.7, 0)) != 0 {
			t.Fatal("empty grid")
		}
		want := .2
		if a != 0 {
			want = math.Exp(2*a*.7) * math.Expm1(2*a*.2) / (2 * a)
		}
		mfNear(t, pricingMust(empty.Variance(.7, 0, .2)), want)
		if pricingMust(empty.Variance(0, 0, math.Nextafter(1, 2)-1)) == 0 {
			t.Fatal("epsilon threshold")
		}
		if pricingMust(empty.Variance(0, 0, (math.Nextafter(1, 2)-1)/2)) != 0 {
			t.Fatal("sub-epsilon step")
		}
		pricingOK(t, empty.Close())
	}
}
