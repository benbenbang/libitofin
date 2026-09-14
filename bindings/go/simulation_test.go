package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestGaussianDeterminism(t *testing.T) {
	a, e := GaussianDraws(100, 42)
	if e != nil {
		t.Fatal(e)
	}
	b, e := GaussianDraws(100, 42)
	if e != nil {
		t.Fatal(e)
	}
	if !reflect.DeepEqual(a, b) {
		t.Fatal("seed changed stream")
	}
	if _, e = GaussianDraws(1, 0); e == nil {
		t.Fatal("accepted zero seed")
	}
}
func TestGBMLayoutAndTerminalConsistency(t *testing.T) {
	c := GBMConfig{Initial: []float64{100, 70}, Drift: []float64{.05, -.02}, Volatility: []float64{.2, .3}, Correlation: []float64{1, .5, .5, 1}, Horizon: 1, Steps: 12, Paths: 100, Seed: 42}
	full, e := SimulateGBM(c)
	if e != nil {
		t.Fatal(e)
	}
	c.TerminalOnly = true
	terminal, e := SimulateGBM(c)
	if e != nil {
		t.Fatal(e)
	}
	for p := 0; p < c.Paths; p++ {
		start := p * 13 * 2
		if !reflect.DeepEqual(full.Values[start:start+2], c.Initial) {
			t.Fatal("initial row changed")
		}
		if !reflect.DeepEqual(full.Values[start+24:start+26], terminal.Values[p*2:p*2+2]) {
			t.Fatal("terminal values changed")
		}
	}
}
func TestGBMDeterministicAndInvalidInputs(t *testing.T) {
	c := GBMConfig{Initial: []float64{100}, Drift: []float64{.05}, Volatility: []float64{0}, Horizon: 1, Steps: 12, Paths: 2, Seed: 42, TerminalOnly: true}
	r, e := SimulateGBM(c)
	if e != nil {
		t.Fatal(e)
	}
	for _, v := range r.Values {
		if v != 100*math.Exp(.05) {
			t.Fatalf("deterministic price %.17g", v)
		}
	}
	c.Correlation = []float64{-1}
	if _, e = SimulateGBM(c); e == nil {
		t.Fatal("accepted invalid correlation")
	}
	c.Correlation = nil
	c.Paths = 100
	c.MaxOutputValues = 2
	if _, e = SimulateGBM(c); e == nil {
		t.Fatal("ignored allocation bound")
	}
}
func TestGBMMeanAndVariance(t *testing.T) {
	c := GBMConfig{Initial: []float64{100}, Drift: []float64{.06}, Volatility: []float64{.25}, Horizon: 1, Steps: 1, Paths: 100000, Seed: 123, TerminalOnly: true}
	r, e := SimulateGBM(c)
	if e != nil {
		t.Fatal(e)
	}
	var mean, ss float64
	for _, v := range r.Values {
		mean += v
	}
	mean /= float64(c.Paths)
	for _, v := range r.Values {
		ss += (v - mean) * (v - mean)
	}
	ss /= float64(c.Paths)
	wantMean := 100 * math.Exp(.06)
	wantVariance := 10000 * math.Exp(.12) * (math.Exp(.25*.25) - 1)
	if math.Abs(mean/wantMean-1) > .003 || math.Abs(ss/wantVariance-1) > .02 {
		t.Fatalf("moments mean=%g variance=%g", mean, ss)
	}
}
