package itofin

import (
	"math"
	"reflect"
	"testing"
)

func hestonConfig() HestonConfig {
	return HestonConfig{
		Spot: 100, Variance: .04, RiskFreeRate: .05, DividendYield: .02,
		Kappa: 1.2, Theta: .06, Sigma: .3, Rho: -.5,
		Horizon: 1, Steps: 4, Paths: 3, Seed: 42, Scheme: HestonQEM,
	}
}

func TestHestonLayoutTerminalSeedAndZeroTime(t *testing.T) {
	c := hestonConfig()
	full, err := SimulateHeston(c)
	if err != nil {
		t.Fatal(err)
	}
	if full.Paths != 3 || full.Times != 5 || full.Assets != 2 || len(full.Values) != 30 {
		t.Fatalf("unexpected shape: %+v", full)
	}
	if !reflect.DeepEqual(full.Values[:2], []float64{100, .04}) {
		t.Fatalf("wrong initial state: %v", full.Values[:2])
	}
	if math.Abs(full.Values[2]-93.21873664131503) > 1e-11 || math.Abs(full.Values[3]-.06567515852602028) > 1e-13 {
		t.Fatalf("fixed QEM fixture: %v", full.Values[2:4])
	}
	again, err := SimulateHeston(c)
	if err != nil || !reflect.DeepEqual(full.Values, again.Values) {
		t.Fatalf("seed reproducibility: %v", err)
	}
	c.TerminalOnly = true
	terminal, err := SimulateHeston(c)
	if err != nil {
		t.Fatal(err)
	}
	for p := 0; p < c.Paths; p++ {
		if !reflect.DeepEqual(terminal.Values[p*2:p*2+2], full.Values[p*10+8:p*10+10]) {
			t.Fatalf("terminal mismatch at path %d", p)
		}
	}
	c.Horizon = 0
	zero, err := SimulateHeston(c)
	if err != nil {
		t.Fatal(err)
	}
	for p := 0; p < c.Paths; p++ {
		if !reflect.DeepEqual(zero.Values[p*2:p*2+2], []float64{100, .04}) {
			t.Fatalf("zero horizon changed path %d", p)
		}
	}
}

func TestHestonSchemesAndInvalidInputs(t *testing.T) {
	c := hestonConfig()
	qem, err := SimulateHeston(c)
	if err != nil {
		t.Fatal(err)
	}
	c.Scheme = HestonQE
	qe, err := SimulateHeston(c)
	if err != nil || reflect.DeepEqual(qe.Values, qem.Values) {
		t.Fatalf("scheme selection: %v", err)
	}
	c.Variance, c.Kappa, c.Theta, c.Sigma = .01, .5, .01, .2
	c.Steps, c.Paths, c.TerminalOnly = 1, 1, true
	high, err := SimulateHeston(c)
	if err != nil {
		t.Fatal(err)
	}
	if math.Abs(high.Values[0]-96.55317400157244) > 1e-11 || math.Abs(high.Values[1]-.018076059597846472) > 1e-13 {
		t.Fatalf("fixed high-psi QE fixture: %v", high.Values)
	}
	bad := []HestonConfig{
		func() HestonConfig { x := c; x.Seed = 0; return x }(),
		func() HestonConfig { x := c; x.Scheme = 2; return x }(),
		func() HestonConfig { x := c; x.Variance = -.01; return x }(),
		func() HestonConfig { x := c; x.Rho = math.NaN(); return x }(),
		func() HestonConfig { x := c; x.MaxOutputValues = 1; return x }(),
	}
	for _, x := range bad {
		if _, err := SimulateHeston(x); err == nil {
			t.Fatalf("accepted invalid config: %+v", x)
		}
	}
}
