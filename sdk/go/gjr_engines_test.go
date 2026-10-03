package itofin

import (
	"math"
	"testing"
)

func gjrBlackPrice(spot, strike, rate, dividend, variance, time float64) float64 {
	stddev := math.Sqrt(variance * time)
	d1 := (math.Log(spot/strike) + (rate-dividend+variance/2)*time) / stddev
	d2 := d1 - stddev
	normal := func(x float64) float64 { return (1 + math.Erf(x/math.Sqrt2)) / 2 }
	return spot*math.Exp(-dividend*time)*normal(d1) - strike*math.Exp(-rate*time)*normal(d2)
}

func TestGJRMCConstantVarianceIndependentBlackPrice(t *testing.T) {
	m := newGJRPricingMarket(t)
	params := GJRParameters{DailyVariance: .04 / 252, Beta: 1, DaysPerYear: 252}
	process := pricingMust(m.s.NewGJRProcess(m.quotes[0], m.risk, m.dividend, params, GJRFullTruncation))
	config := GJRMCConfig{Steps: 1, Samples: 32768, Seed: 42, Antithetic: true}
	engine := pricingMust(m.s.NewMCGJREngine(process, config))
	value := pricingMust(m.option.PriceMCGJR(engine))
	errorEstimate := pricingMust(m.option.ErrorEstimate())
	if errorEstimate <= 0 || math.IsNaN(errorEstimate) {
		t.Fatal("invalid MC error estimate", errorEstimate)
	}
	want := gjrBlackPrice(100, 100, .05, .02, .04, 1)
	pricingNear(t, value, want, 4*errorEstimate)
	duplicate := pricingMust(m.s.NewMCGJREngine(process, config))
	pricingNear(t, pricingMust(m.option.PriceMCGJR(duplicate)), value, 0)
	pricingNear(t, pricingMust(m.option.ErrorEstimate()), errorEstimate, 0)
	config.Seed = 43
	different := pricingMust(m.s.NewMCGJREngine(process, config))
	if pricingMust(m.option.PriceMCGJR(different)) == value {
		t.Fatal("seed ignored")
	}
	pricingOK(t, m.option.SetMCGJREngine(engine))
	pricingOK(t, m.quotes[0].SetValue(105))
	updated := pricingMust(m.option.NPV())
	if updated == value {
		t.Fatal("MC retained market update ignored")
	}
	for _, q := range m.quotes[1:] {
		pricingOK(t, q.Close())
	}
	for _, close := range []func() error{m.risk.Close, m.dividend.Close, process.Close, engine.Close, duplicate.Close, different.Close, m.process.Close, m.model.Close, m.engine.Close, m.settings.Close} {
		pricingOK(t, close())
	}
	pricingOK(t, m.quotes[0].SetValue(110))
	if pricingMust(m.option.IsCalculated()) {
		t.Fatal("post-close quote update did not invalidate MC cache")
	}
	reference := newGJRPricingMarket(t)
	pricingOK(t, reference.quotes[0].SetValue(110))
	referenceProcess := pricingMust(reference.s.NewGJRProcess(reference.quotes[0], reference.risk, reference.dividend, params, GJRFullTruncation))
	config.Seed = 42
	referenceEngine := pricingMust(reference.s.NewMCGJREngine(referenceProcess, config))
	fresh := pricingMust(reference.option.PriceMCGJR(referenceEngine))
	if fresh == updated {
		t.Fatal("fresh MC reference ignored spot update")
	}
	pricingNear(t, pricingMust(m.option.NPV()), fresh, 0)
	pricingNear(t, pricingMust(m.option.ErrorEstimate()), pricingMust(reference.option.ErrorEstimate()), 0)
	pricingOK(t, m.quotes[0].Close())
	if _, err := m.option.Delta(); err == nil {
		t.Fatal("fabricated MC Greek")
	}
}

func TestGJRMCAllSchemesAndToleranceBudget(t *testing.T) {
	m := newGJRPricingMarket(t)
	for _, scheme := range []GJRScheme{GJRPartialTruncation, GJRFullTruncation, GJRReflection} {
		process := pricingMust(m.s.NewGJRProcess(m.quotes[0], m.risk, m.dividend, m.params, scheme))
		engine := pricingMust(m.s.NewMCGJREngine(process, GJRMCConfig{StepsPerYear: 252, Samples: 2048, Seed: 42, Antithetic: true}))
		if value := pricingMust(m.option.PriceMCGJR(engine)); value <= 0 || math.IsNaN(value) {
			t.Fatal("invalid scheme price", scheme, value)
		}
		if pricingMust(process.Discretization()) != scheme {
			t.Fatal("MC modified process scheme")
		}
	}
	engine := pricingMust(m.s.NewMCGJREngine(m.process, GJRMCConfig{Steps: 16, AbsoluteTolerance: .5, MaxSamples: 4096, Seed: 42, Antithetic: true}))
	pricingMust(m.option.PriceMCGJR(engine))
	if pricingMust(m.option.ErrorEstimate()) > .5 {
		t.Fatal("requested tolerance missed")
	}
	budget := pricingMust(m.s.NewMCGJREngine(m.process, GJRMCConfig{Steps: 16, AbsoluteTolerance: 1e-12, MaxSamples: 1023, Seed: 42}))
	if _, err := m.option.PriceMCGJR(budget); err == nil {
		t.Fatal("exhausted tolerance budget accepted")
	}
	american := pricingMust(m.s.NewAmericanOption(Put, 100, m.today, m.expiry, m.settings))
	if _, err := american.PriceMCGJR(engine); err == nil {
		t.Fatal("American MC payoff accepted")
	}
}

func TestGJRMCConfigurationBoundaries(t *testing.T) {
	m := newGJRPricingMarket(t)
	for _, cfg := range []GJRMCConfig{
		{}, {Steps: 1}, {Samples: 2}, {Steps: 1, StepsPerYear: 252, Samples: 2},
		{Steps: 1, Samples: 2, AbsoluteTolerance: .1}, {Steps: 1, AbsoluteTolerance: -.1},
		{Steps: 1, AbsoluteTolerance: math.NaN()}, {Steps: 1, AbsoluteTolerance: math.Inf(1)},
		{Steps: 1, Samples: 1}, {Steps: 1, Samples: 2, Seed: 1 << 32},
		{Steps: 100001, Samples: 2}, {Steps: 1, Samples: 1000001},
		{Steps: 100000, Samples: 1000, Antithetic: true},
		{Steps: 1, AbsoluteTolerance: .1, MaxSamples: 1022},
	} {
		if _, err := m.s.NewMCGJREngine(m.process, cfg); err == nil {
			t.Fatal("invalid configuration accepted", cfg)
		}
	}
	pricingOK(t, m.process.Close())
	if _, err := m.s.NewMCGJREngine(m.process, GJRMCConfig{Steps: 1, Samples: 2}); err == nil {
		t.Fatal("closed process accepted")
	}
}
