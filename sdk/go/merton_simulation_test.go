package itofin

import (
	"math"
	"reflect"
	"testing"
)

func mertonPathConfig() MertonConfig {
	return MertonConfig{
		Spot: 100, Drift: .05, Volatility: .2, JumpIntensity: 1,
		LogMeanJump: -.1, LogJumpVolatility: .3, Horizon: 1,
		Steps: 4, Paths: 3, Seed: 42,
	}
}

func TestMertonSimulationLayoutSeedTerminalAndOwnedOutput(t *testing.T) {
	c := mertonPathConfig()
	full := pricingMust(SimulateMerton(c))
	if full.Paths != 3 || full.Times != 5 || full.Assets != 1 || full.TerminalOnly || len(full.Values) != 15 {
		t.Fatalf("wrong output metadata: %+v", full)
	}
	for path := 0; path < 3; path++ {
		if full.Values[path*5] != 100 {
			t.Fatal("initial spot changed")
		}
	}
	again := pricingMust(SimulateMerton(c))
	if !reflect.DeepEqual(full.Values, again.Values) {
		t.Fatal("seed did not repeat")
	}
	c.TerminalOnly = true
	terminal := pricingMust(SimulateMerton(c))
	if terminal.Times != 1 || !terminal.TerminalOnly || len(terminal.Values) != 3 {
		t.Fatal("wrong terminal metadata")
	}
	for path := 0; path < 3; path++ {
		if math.Float64bits(terminal.Values[path]) != math.Float64bits(full.Values[path*5+4]) {
			t.Fatal("terminal differs from full path")
		}
	}
	c.Seed++
	if reflect.DeepEqual(terminal.Values, pricingMust(SimulateMerton(c)).Values) {
		t.Fatal("changing seed did not change paths")
	}
	full.Values[1] = -1
	if again.Values[1] == -1 {
		t.Fatal("outputs share backing storage")
	}
	c.Horizon = 0
	for _, terminalOnly := range []bool{false, true} {
		c.TerminalOnly = terminalOnly
		for _, value := range pricingMust(SimulateMerton(c)).Values {
			if value != c.Spot {
				t.Fatal("zero horizon changed spot")
			}
		}
	}
	c.Horizon = 1
	c.Seed = ^uint32(0)
	if _, err := SimulateMerton(c); err != nil {
		t.Fatalf("maximum nonzero seed: %v", err)
	}
}

func TestMertonZeroIntensityMatchesScalarGBMExactly(t *testing.T) {
	c := mertonPathConfig()
	c.JumpIntensity = 0
	for _, volatility := range []float64{.2, 0} {
		c.Volatility = volatility
		for _, terminal := range []bool{false, true} {
			c.TerminalOnly = terminal
			merton := pricingMust(SimulateMerton(c))
			gbm := pricingMust(SimulateGBM(GBMConfig{
				Initial: []float64{c.Spot}, Drift: []float64{c.Drift}, Volatility: []float64{c.Volatility},
				Horizon: c.Horizon, Steps: c.Steps, Paths: c.Paths, Seed: c.Seed, TerminalOnly: terminal,
			}))
			if !reflect.DeepEqual(merton.Values, gbm.Values) {
				t.Fatal("zero intensity differs from scalar GBM")
			}
		}
	}
}

func TestMertonSimulationDomainOverflowLimitsAndRecovery(t *testing.T) {
	invalid := []struct {
		name   string
		change func(*MertonConfig)
	}{
		{"spot_zero", func(c *MertonConfig) { c.Spot = 0 }},
		{"spot_nonfinite", func(c *MertonConfig) { c.Spot = math.Inf(1) }},
		{"drift_nan", func(c *MertonConfig) { c.Drift = math.NaN() }},
		{"vol_negative", func(c *MertonConfig) { c.Volatility = -1 }},
		{"vol_nonfinite", func(c *MertonConfig) { c.Volatility = math.Inf(1) }},
		{"intensity_negative", func(c *MertonConfig) { c.JumpIntensity = -1 }},
		{"intensity_nan", func(c *MertonConfig) { c.JumpIntensity = math.NaN() }},
		{"mean_nonfinite", func(c *MertonConfig) { c.LogMeanJump = math.Inf(1) }},
		{"dispersion_negative", func(c *MertonConfig) { c.LogJumpVolatility = -1 }},
		{"dispersion_nan", func(c *MertonConfig) { c.LogJumpVolatility = math.NaN() }},
		{"horizon_negative", func(c *MertonConfig) { c.Horizon = -1 }},
		{"horizon_nonfinite", func(c *MertonConfig) { c.Horizon = math.Inf(1) }},
		{"steps_zero", func(c *MertonConfig) { c.Steps = 0 }},
		{"paths_negative", func(c *MertonConfig) { c.Paths = -1 }},
		{"seed_zero", func(c *MertonConfig) { c.Seed = 0 }},
		{"negative_limit", func(c *MertonConfig) { c.MaxOutputValues = -1 }},
		{"small_limit", func(c *MertonConfig) { c.MaxOutputValues = 14 }},
		{"default_limit", func(c *MertonConfig) { c.TerminalOnly = true; c.Steps = 1; c.Paths = DefaultMaxOutputValues + 1 }},
		{"work_overflow", func(c *MertonConfig) { c.TerminalOnly = true; c.Paths = 1; c.Steps = int(^uint(0) >> 1) }},
		{"draw_overflow", func(c *MertonConfig) { c.TerminalOnly = true; c.Steps = 1; c.Paths = int(^uint(0)>>1)/3 + 1 }},
		{"byte_overflow", func(c *MertonConfig) {
			c.TerminalOnly = true
			c.Steps = 1
			c.Paths = int(^uint(0)>>1)/8 + 1
			c.MaxOutputValues = int(^uint(0) >> 1)
		}},
		{"step_underflow", func(c *MertonConfig) { c.Horizon = math.SmallestNonzeroFloat64 }},
		{"poisson_underflow", func(c *MertonConfig) { c.JumpIntensity = math.SmallestNonzeroFloat64 }},
		{"poisson_recurrence_domain", func(c *MertonConfig) { c.Steps = 1; c.JumpIntensity = 1000 }},
		{"diffusion_variance_overflow", func(c *MertonConfig) { c.Volatility = 1e200 }},
		{"jump_multiplier_underflow", func(c *MertonConfig) { c.LogMeanJump = -1000 }},
		{"late_path_underflow", func(c *MertonConfig) { c.Drift = -1000; c.Volatility = 0; c.JumpIntensity = 0 }},
		{"jump_variance_overflow", func(c *MertonConfig) { c.LogJumpVolatility = 1e200 }},
		{"jump_compensation_overflow", func(c *MertonConfig) { c.LogMeanJump = 1000 }},
		{"late_path_overflow", func(c *MertonConfig) { c.Drift = 1000; c.Volatility = 0; c.JumpIntensity = 0 }},
	}
	for _, test := range invalid {
		t.Run(test.name, func(t *testing.T) {
			c := mertonPathConfig()
			test.change(&c)
			if result, err := SimulateMerton(c); err == nil || result != nil {
				t.Fatal("invalid request accepted or partial result returned")
			}
		})
	}
	c := mertonPathConfig()
	c.MaxOutputValues = 15
	if _, err := SimulateMerton(c); err != nil {
		t.Fatalf("exact output limit: %v", err)
	}
	c.TerminalOnly = true
	c.MaxOutputValues = 3
	if _, err := SimulateMerton(c); err != nil {
		t.Fatalf("terminal output limit: %v", err)
	}
}

func TestMertonSimulationsRunConcurrentlyWithoutSession(t *testing.T) {
	c := mertonPathConfig()
	want := pricingMust(SimulateMerton(c)).Values
	type result struct {
		simulation *Simulation
		err        error
	}
	results := make(chan result, 8)
	for range 8 {
		go func() { simulation, err := SimulateMerton(c); results <- result{simulation, err} }()
	}
	for range 8 {
		r := <-results
		if r.err != nil {
			t.Fatal(r.err)
		}
		if !reflect.DeepEqual(r.simulation.Values, want) {
			t.Fatal("concurrent call changed random stream")
		}
	}
}
