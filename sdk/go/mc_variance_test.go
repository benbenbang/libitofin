package itofin

import (
	"math"
	"strings"
	"testing"
)

const (
	mcBridgeNPV = 9.271732701042982
	mcCVNPV     = 0.7231366070567805
)

func mcVarianceEuropean(t *testing.T, s *Session) (*BlackScholesProcess, *Settings, Date) {
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: pricingMust(s.Actual360())}))
	return p, settings, pricingMust(today.AddDays(360))
}

func mcBridgeConfig(steps uint, bridge bool) MCConfig {
	return MCConfig{Steps: pricingPtr(steps), Samples: pricingPtr(uint(40000)), Seed: pricingPtr(uint32(42)), BrownianBridge: pricingPtr(bridge)}
}

// Core #1362 oracle: 3*SE band at 12 steps, a changed stream, identity on one step.
func TestMCEuropeanBrownianBridgeOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	p, settings, expiry := mcVarianceEuropean(t, s)
	opt := pricingMust(s.NewVanillaOption(Call, 100, expiry, settings))
	analytic := pricingMust(opt.Price(p))
	price := func(steps uint, bridge bool) float64 {
		return pricingMust(opt.PriceMC(pricingMust(s.NewMCEuropeanEngine(p, mcBridgeConfig(steps, bridge)))))
	}
	bridged := price(12, true)
	se := pricingMust(opt.ErrorEstimate())
	if bridged != mcBridgeNPV || se <= 0 || math.Abs(bridged-analytic) >= 3*se {
		t.Fatalf("bridged %.17g SE %g analytic %g", bridged, se, analytic)
	}
	if price(12, false) == bridged {
		t.Fatal("bridge did not change the stream")
	}
	pricingNear(t, price(1, true), price(1, false), 1e-12*bridged)
	qmc := func(steps uint, bridge bool, seed uint32) float64 {
		cfg := MCConfig{Steps: pricingPtr(steps), Samples: pricingPtr(uint(4095)), Seed: pricingPtr(seed), BrownianBridge: pricingPtr(bridge)}
		return pricingMust(opt.PriceQMC(pricingMust(s.NewQMCEuropeanEngine(p, cfg))))
	}
	sobol := qmc(40, true, 0)
	pricingNear(t, sobol, qmc(1, false, 0), 1e-9)
	pricingNear(t, qmc(40, true, 7), sobol, 1e-9)
	if math.Abs(qmc(40, false, 0)-sobol) <= 1e-9 {
		t.Fatal("QMC bridge did not change the terminal draw")
	}
}

// Core #1363 oracle on the mclongstaffschwartzengine.cpp fixture: fixed samples,
// unchanged exercise probability, smaller error for the K=32 antithetic put.
// Each engine outlives its closed process.
func TestMCAmericanControlVariateOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settlement := pricingMust(NewDate(17, 5, 1998))
	expiry := pricingMust(NewDate(17, 5, 1999))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(pricingMust(NewDate(15, 5, 1998))))
	opt := pricingMust(s.NewAmericanOption(Put, 32, settlement, expiry, settings))
	run := func(cv bool) (float64, float64, float64) {
		p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 36, RiskFreeRate: .06, Volatility: .2, ReferenceDate: settlement, DayCounter: pricingMust(s.Actual365Fixed())}))
		cfg := MCConfig{Steps: pricingPtr(uint(75)), Samples: pricingPtr(uint(4096)), Seed: pricingPtr(uint32(42)), Antithetic: pricingPtr(true), PolynomialOrder: pricingPtr(uint(3)), ControlVariate: pricingPtr(cv)}
		engine := pricingMust(s.NewMCAmericanEngine(p, cfg))
		pricingOK(t, p.Close())
		value := pricingMust(opt.PriceMCAmerican(engine))
		return value, pricingMust(opt.ErrorEstimate()), pricingMust(opt.ExerciseProbability())
	}
	value, se, probability := run(true)
	if value != mcCVNPV {
		t.Fatalf("control variate value %.17g", value)
	}
	plain, plainSE, plainProbability := run(false)
	if plain == value || se >= plainSE || probability != plainProbability {
		t.Fatalf("cv %g/%g/%g plain %g/%g/%g", value, se, probability, plain, plainSE, plainProbability)
	}
}

func TestMCVarianceReductionLifetimeAndRejections(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	p, settings, expiry := mcVarianceEuropean(t, s)
	engine := pricingMust(s.NewMCEuropeanEngine(p, mcBridgeConfig(12, true)))
	pricingOK(t, p.Close())
	opt := pricingMust(s.NewVanillaOption(Call, 100, expiry, settings))
	if got := pricingMust(opt.PriceMC(engine)); got != mcBridgeNPV {
		t.Fatalf("retained engine %.17g", got)
	}
	p, _, _ = mcVarianceEuropean(t, s)
	heston := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .05, Spot: 100, V0: .04, Kappa: 1, Theta: .04, Sigma: .3, Rho: -.5, ReferenceDate: pricingMust(NewDate(15, 6, 2026)), DayCounter: pricingMust(s.Actual360())}))
	cv := MCConfig{Steps: pricingPtr(uint(1)), Samples: pricingPtr(uint(4095)), ControlVariate: pricingPtr(true)}
	bridge := MCConfig{Steps: pricingPtr(uint(1)), Samples: pricingPtr(uint(4095)), BrownianBridge: pricingPtr(true)}
	for _, c := range []struct {
		want  string
		build func() error
	}{
		{"control variate requires", func() error { _, err := s.NewMCEuropeanEngine(p, cv); return err }},
		{"control variate requires", func() error { _, err := s.NewQMCEuropeanEngine(p, cv); return err }},
		{"brownian bridge requires", func() error { _, err := s.NewMCEuropeanHestonEngine(heston, bridge); return err }},
		{"brownian bridge requires", func() error { _, err := s.NewMCAmericanEngine(p, bridge); return err }},
		{"brownian bridge requires", func() error { _, err := s.NewMCAmericanEngineWithBasis(p, bridge, Monomial); return err }},
	} {
		if err := c.build(); err == nil || !strings.Contains(err.Error(), c.want) {
			t.Fatalf("want %q, got %v", c.want, err)
		}
	}
}
