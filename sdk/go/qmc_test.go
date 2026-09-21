package itofin

import (
	"errors"
	"math"
	"strconv"
	"testing"
)

func qmcConfig() MCConfig {
	return MCConfig{Steps: pricingPtr(uint(1)), Samples: pricingPtr(uint(4095))}
}

func TestQMCEuropeanQuantLib108Cases(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	expiry := pricingMust(today.AddDays(360))
	dc := pricingMust(s.Actual360())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	cases := 0
	for _, kind := range []OptionType{Call, Put} {
		for _, strike := range []float64{75, 100, 125} {
			for _, dividend := range []float64{0, .05} {
				for _, rate := range []float64{.01, .05, .15} {
					for _, vol := range []float64{.11, .50, 1.20} {
						p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: rate, DividendYield: dividend, Volatility: vol, ReferenceDate: today, DayCounter: dc}))
						option := pricingMust(s.NewVanillaOption(kind, strike, expiry, settings))
						analytic := pricingMust(option.Price(p))
						engine := pricingMust(s.NewQMCEuropeanEngine(p, qmcConfig()))
						qmc := pricingMust(option.PriceQMC(engine))
						if math.IsNaN(analytic) || math.IsInf(analytic, 0) || math.IsNaN(qmc) || math.IsInf(qmc, 0) || math.Abs(qmc-analytic)/100 > .01 {
							t.Fatalf("QMC %g analytic %g, kind=%v strike=%g q=%g r=%g vol=%g", qmc, analytic, kind, strike, dividend, rate, vol)
						}
						repeat := pricingMust(s.NewQMCEuropeanEngine(p, qmcConfig()))
						if got := pricingMust(option.PriceQMC(repeat)); got != qmc {
							t.Fatalf("QMC repeat %g != %g", got, qmc)
						}
						if _, err := option.ErrorEstimate(); err == nil {
							t.Fatal("QMC supplied a statistical error estimate")
						}
						for _, o := range []object{p.object, option.object, engine.object, repeat.object} {
							pricingOK(t, o.Close())
						}
						cases++
					}
				}
			}
		}
	}
	if cases != 108 {
		t.Fatalf("ran %d cases", cases)
	}
}

func TestQMCEuropeanAntitheticAndZeroSeed(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: pricingMust(s.Actual360())}))
	option := pricingMust(s.NewVanillaOption(Call, 100, pricingMust(today.AddDays(360)), settings))
	cfg := qmcConfig()
	implicit := pricingMust(option.PriceQMC(pricingMust(s.NewQMCEuropeanEngine(p, cfg))))
	cfg.Seed = pricingPtr(uint32(0))
	if got := pricingMust(option.PriceQMC(pricingMust(s.NewQMCEuropeanEngine(p, cfg)))); got != implicit {
		t.Fatal("explicit zero seed changed QMC sequence")
	}
	cfg.Antithetic = pricingPtr(true)
	antithetic := pricingMust(option.PriceQMC(pricingMust(s.NewQMCEuropeanEngine(p, cfg))))
	analytic := pricingMust(option.Price(p))
	if math.IsNaN(analytic) || math.IsInf(analytic, 0) || math.IsNaN(antithetic) || math.IsInf(antithetic, 0) || math.Abs(antithetic-analytic)/100 > .01 {
		t.Fatalf("antithetic %g analytic %g", antithetic, analytic)
	}
}

func TestQMCEuropeanLiveQuotesAndRetainedInputs(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	expiry := pricingMust(today.AddDays(360))
	dc := pricingMust(s.Actual360())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	quote := pricingMust(s.NewSimpleQuote(.05))
	rate := pricingMust(s.NewFlatForwardFromQuote(today, quote, dc))
	dividend := pricingMust(s.NewFlatForward(today, .02, dc))
	vol := pricingMust(s.BlackConstantVol(BlackConstantVolConfig{ReferenceDate: today, Volatility: .2, DayCounter: dc}))
	p := pricingMust(s.NewBlackScholesProcessFromCurves(100, rate, dividend, vol))
	engine := pricingMust(s.NewQMCEuropeanEngine(p, qmcConfig()))
	option := pricingMust(s.NewVanillaOption(Call, 100, expiry, settings))
	pricingOK(t, option.SetQMCEngine(engine))
	for _, o := range []object{rate.object, dividend.object, vol.object, p.object, engine.object, settings.object} {
		pricingOK(t, o.Close())
	}
	initial := pricingMust(option.NPV())
	pricingOK(t, quote.SetValue(.08))
	updated := pricingMust(option.NPV())
	if math.IsNaN(initial) || math.IsInf(initial, 0) || math.IsNaN(updated) || math.IsInf(updated, 0) || initial == updated {
		t.Fatal("live rate did not reprice retained engine")
	}
	freshSettings := pricingMust(s.NewSettings())
	pricingOK(t, freshSettings.SetEvaluationDate(today))
	fresh := pricingMust(s.NewVanillaOption(Call, 100, expiry, freshSettings))
	freshP := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .08, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	freshEngine := pricingMust(s.NewQMCEuropeanEngine(freshP, qmcConfig()))
	if got := pricingMust(fresh.PriceQMC(freshEngine)); got != updated {
		t.Fatalf("updated %g fresh %g", updated, got)
	}
	pricingOK(t, quote.SetValue(.05))
	pricingOK(t, quote.Close())
	pricingOK(t, dc.Close())
	if got := pricingMust(option.NPV()); got != initial {
		t.Fatalf("restored %g initial %g", got, initial)
	}
}

func TestQMCEuropeanInvalidConfigurationAndRecovery(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	dc := pricingMust(s.Actual360())
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	for _, change := range []func(*MCConfig){
		func(c *MCConfig) { c.Steps = nil },
		func(c *MCConfig) { c.Samples = nil },
		func(c *MCConfig) { c.Steps = pricingPtr(uint(0)) },
		func(c *MCConfig) { c.Samples = pricingPtr(uint(0)) },
		func(c *MCConfig) { c.StepsPerYear = pricingPtr(uint(1)) },
		func(c *MCConfig) { c.Steps = nil; c.StepsPerYear = pricingPtr(uint(0)) },
		func(c *MCConfig) { c.AbsoluteTolerance = pricingPtr(.01) },
		func(c *MCConfig) { c.Samples = nil; c.AbsoluteTolerance = pricingPtr(.01) },
		func(c *MCConfig) { c.MaxSamples = pricingPtr(uint(8191)) },
		func(c *MCConfig) { c.PolynomialOrder = pricingPtr(uint(2)) },
	} {
		cfg := qmcConfig()
		change(&cfg)
		if _, err := s.NewQMCEuropeanEngine(p, cfg); err == nil {
			t.Fatalf("invalid config accepted: %+v", cfg)
		}
	}
	if strconv.IntSize > 32 {
		tooMany := uint64(1) << 32
		cfg := qmcConfig()
		cfg.Samples = pricingPtr(uint(tooMany))
		if _, err := s.NewQMCEuropeanEngine(p, cfg); err == nil {
			t.Fatal("Sobol period exceeded")
		}
	}
	if _, err := s.NewQMCEuropeanEngine(nil, qmcConfig()); err == nil {
		t.Fatal("nil process accepted")
	}
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewQMCEuropeanEngine(p, qmcConfig()); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session: %v", err)
	}
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	option := pricingMust(s.NewVanillaOption(Call, 100, pricingMust(today.AddDays(360)), settings))
	if err := option.SetQMCEngine(nil); err == nil {
		t.Fatal("nil engine accepted")
	}
	if _, err := option.PriceQMC(nil); err == nil {
		t.Fatal("nil engine accepted for pricing")
	}
	cfg := qmcConfig()
	cfg.Steps, cfg.StepsPerYear = nil, pricingPtr(uint(1))
	engine := pricingMust(s.NewQMCEuropeanEngine(p, cfg))
	value := pricingMust(option.PriceQMC(engine))
	if math.IsNaN(value) || math.IsInf(value, 0) {
		t.Fatal("invalid calls prevented recovery")
	}
	pricingOK(t, engine.Close())
	if err := option.SetQMCEngine(engine); err == nil {
		t.Fatal("closed engine accepted")
	}
}
