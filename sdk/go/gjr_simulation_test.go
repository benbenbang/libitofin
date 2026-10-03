package itofin

import (
	"math"
	"reflect"
	"testing"
)

func gjrPathConfig() GJRConfig {
	return GJRConfig{
		Spot: 100, DailyVariance: .00016, RiskFreeRate: .05, DividendYield: .02,
		Omega: .000002, Alpha: .04, Beta: .9, Gamma: .06, Lambda: .1, DaysPerYear: 252,
		Horizon: 1, Steps: 4, Paths: 3, Seed: 42, Scheme: GJRFullTruncation,
	}
}

func TestGJRSimulationLayoutTerminalPrefixAndSeed(t *testing.T) {
	for _, scheme := range []GJRScheme{GJRPartialTruncation, GJRFullTruncation, GJRReflection} {
		c := gjrPathConfig()
		c.Scheme = scheme
		full := pricingMust(SimulateGJR(c))
		if full.Paths != 3 || full.Times != 5 || full.Assets != 2 || full.TerminalOnly || len(full.Values) != 30 {
			t.Fatalf("wrong output metadata: %+v", full)
		}
		for path := range 3 {
			if full.Values[path*10] != c.Spot || full.Values[path*10+1] != c.DailyVariance*c.DaysPerYear {
				t.Fatal("initial spot or annualized variance changed")
			}
		}
		again := pricingMust(SimulateGJR(c))
		if !reflect.DeepEqual(full.Values, again.Values) {
			t.Fatal("seed did not repeat")
		}
		c.Paths = 2
		if !reflect.DeepEqual(full.Values[:20], pricingMust(SimulateGJR(c)).Values) {
			t.Fatal("larger batch changed path prefix")
		}
		c.Paths = 3
		c.TerminalOnly = true
		terminal := pricingMust(SimulateGJR(c))
		if terminal.Times != 1 || !terminal.TerminalOnly || terminal.Assets != 2 || len(terminal.Values) != 6 {
			t.Fatal("wrong terminal metadata")
		}
		for path := range 3 {
			for asset := range 2 {
				if math.Float64bits(terminal.Values[path*2+asset]) != math.Float64bits(full.Values[path*10+8+asset]) {
					t.Fatal("terminal differs from full path")
				}
			}
		}
		c.Seed++
		if reflect.DeepEqual(terminal.Values, pricingMust(SimulateGJR(c)).Values) {
			t.Fatal("changing seed did not change paths")
		}
		full.Values[2] = -1
		if again.Values[2] == -1 {
			t.Fatal("outputs share backing storage")
		}
		c.Horizon = 0
		for _, terminalOnly := range []bool{false, true} {
			c.TerminalOnly = terminalOnly
			for i, value := range pricingMust(SimulateGJR(c)).Values {
				want := c.Spot
				if i%2 == 1 {
					want = c.DailyVariance * c.DaysPerYear
				}
				if value != want {
					t.Fatal("zero horizon changed state")
				}
			}
		}
	}
}

func TestGJRSimulationDomainOverflowAndBudget(t *testing.T) {
	invalid := []struct {
		name   string
		change func(*GJRConfig)
	}{
		{"spot_zero", func(c *GJRConfig) { c.Spot = 0 }},
		{"spot_inf", func(c *GJRConfig) { c.Spot = math.Inf(1) }},
		{"variance_negative", func(c *GJRConfig) { c.DailyVariance = -1 }},
		{"variance_nan", func(c *GJRConfig) { c.DailyVariance = math.NaN() }},
		{"rate_nan", func(c *GJRConfig) { c.RiskFreeRate = math.NaN() }},
		{"dividend_inf", func(c *GJRConfig) { c.DividendYield = math.Inf(1) }},
		{"omega_negative", func(c *GJRConfig) { c.Omega = -1 }},
		{"alpha_negative", func(c *GJRConfig) { c.Alpha = -1 }},
		{"beta_negative", func(c *GJRConfig) { c.Beta = -1 }},
		{"gamma_domain", func(c *GJRConfig) { c.Gamma = -.1 }},
		{"lambda_inf", func(c *GJRConfig) { c.Lambda = math.Inf(1) }},
		{"days_zero", func(c *GJRConfig) { c.DaysPerYear = 0 }},
		{"days_negative", func(c *GJRConfig) { c.DaysPerYear = -1 }},
		{"horizon_negative", func(c *GJRConfig) { c.Horizon = -1 }},
		{"horizon_nan", func(c *GJRConfig) { c.Horizon = math.NaN() }},
		{"steps_zero", func(c *GJRConfig) { c.Steps = 0 }},
		{"paths_negative", func(c *GJRConfig) { c.Paths = -1 }},
		{"seed_zero", func(c *GJRConfig) { c.Seed = 0 }},
		{"scheme_negative", func(c *GJRConfig) { c.Scheme = -1 }},
		{"scheme_unknown", func(c *GJRConfig) { c.Scheme = 3 }},
		{"negative_limit", func(c *GJRConfig) { c.MaxOutputValues = -1 }},
		{"small_limit", func(c *GJRConfig) { c.MaxOutputValues = 29 }},
		{"default_limit", func(c *GJRConfig) { c.TerminalOnly = true; c.Steps = 1; c.Paths = DefaultMaxOutputValues/2 + 1 }},
		{"work_overflow", func(c *GJRConfig) { c.TerminalOnly = true; c.Steps = int(^uint(0) >> 1); c.Paths = 1 }},
		{"path_overflow", func(c *GJRConfig) { c.TerminalOnly = true; c.Steps = 1; c.Paths = int(^uint(0)>>1)/2 + 1 }},
		{"byte_overflow", func(c *GJRConfig) {
			c.TerminalOnly = true
			c.Steps = 1
			c.Paths = int(^uint(0)>>1)/16 + 1
			c.MaxOutputValues = int(^uint(0) >> 1)
		}},
		{"variance_overflow", func(c *GJRConfig) { c.DailyVariance = 1e307 }},
		{"step_underflow", func(c *GJRConfig) { c.Horizon = math.SmallestNonzeroFloat64 }},
		{"rate_spread_overflow", func(c *GJRConfig) { c.RiskFreeRate = math.MaxFloat64; c.DividendYield = -math.MaxFloat64 }},
		{"late_spot_overflow", func(c *GJRConfig) { c.RiskFreeRate = 1000 }},
		{"late_spot_underflow", func(c *GJRConfig) { c.RiskFreeRate = -1000 }},
	}
	for _, test := range invalid {
		t.Run(test.name, func(t *testing.T) {
			c := gjrPathConfig()
			test.change(&c)
			if result, err := SimulateGJR(c); err == nil || result != nil {
				t.Fatal("invalid request accepted or partial output returned")
			}
		})
	}
	c := gjrPathConfig()
	c.MaxOutputValues = 30
	if _, err := SimulateGJR(c); err != nil {
		t.Fatalf("exact full output limit: %v", err)
	}
	c.TerminalOnly = true
	c.MaxOutputValues = 6
	c.Seed = ^uint32(0)
	if _, err := SimulateGJR(c); err != nil {
		t.Fatalf("exact terminal output limit and maximum seed: %v", err)
	}
	c.DailyVariance = 0
	c.Omega = 0
	c.Alpha = 0
	c.Beta = 1
	c.Gamma = 0
	c.MaxOutputValues = 0
	for _, value := range pricingMust(SimulateGJR(c)).Values {
		if math.IsNaN(value) || math.IsInf(value, 0) {
			t.Fatal("valid degenerate configuration failed")
		}
	}
}

func TestGJRSimulationConcurrentCalls(t *testing.T) {
	c := gjrPathConfig()
	want := pricingMust(SimulateGJR(c)).Values
	type result struct {
		simulation *Simulation
		err        error
	}
	results := make(chan result, 8)
	for range 8 {
		go func() { simulation, err := SimulateGJR(c); results <- result{simulation, err} }()
	}
	for range 8 {
		r := <-results
		if r.err != nil {
			t.Fatal(r.err)
		}
		if !reflect.DeepEqual(r.simulation.Values, want) {
			t.Fatal("concurrent call changed stream")
		}
	}
}
