package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"testing"
)

func mcDepthConfig() MCConfig {
	return MCConfig{Steps: pricingPtr(uint(25)), Samples: pricingPtr(uint(2048)), Seed: pricingPtr(uint32(42)), Antithetic: pricingPtr(true), PolynomialOrder: pricingPtr(uint(3)), CalibrationSamples: pricingPtr(uint(2048))}
}

func TestMCAmericanBasisAndLatestOnly(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(17, 5, 1998))
	expiry := pricingMust(NewDate(17, 5, 1999))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 36, RiskFreeRate: .06, Volatility: .2, ReferenceDate: today, DayCounter: pricingMust(s.Actual365Fixed())}))
	option := pricingMust(s.NewAmericanOptionUntil(Put, 36, expiry, settings))
	explicit := pricingMust(s.NewAmericanOption(Put, 36, today, expiry, settings))
	baseline := pricingMust(explicit.PriceMCAmerican(pricingMust(s.NewMCAmericanEngine(p, mcDepthConfig()))))
	for _, basis := range []PolynomialType{Monomial, Laguerre, Hermite, Hyperbolic, Chebyshev2nd} {
		engine := pricingMust(s.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), basis))
		value := pricingMust(option.PriceMCAmerican(engine))
		if math.IsNaN(value) || math.IsInf(value, 0) || (basis == Monomial && value != baseline) || (basis != Monomial && value == baseline) {
			t.Fatalf("basis %v value %g baseline %g", basis, value, baseline)
		}
		repeat := pricingMust(s.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), basis))
		if got := pricingMust(option.PriceMCAmerican(repeat)); got != value {
			t.Fatalf("basis %v did not repeat: %g != %g", basis, got, value)
		}
	}
	for _, basis := range []PolynomialType{-1, Legendre, Chebyshev, 7} {
		if _, err := s.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), basis); err == nil {
			t.Fatalf("invalid basis %v accepted", basis)
		}
	}
	if _, err := s.NewMCAmericanEngineWithBasis(nil, mcDepthConfig(), Monomial); err == nil {
		t.Fatal("nil process accepted")
	}
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), Monomial); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session engine: %v", err)
	}
	if _, err := other.NewAmericanOptionUntil(Put, 36, expiry, settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session settings: %v", err)
	}
	if _, err := s.NewAmericanOptionUntil(Put, 36, expiry, nil); err == nil {
		t.Fatal("nil settings accepted")
	}
	if got := pricingMust(option.PriceMCAmerican(pricingMust(s.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), Monomial)))); got != baseline {
		t.Fatal("invalid calls prevented recovery")
	}
}

func TestMCBermudanRetentionUpdatesAndErrors(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(17, 5, 1998))
	expiry := pricingMust(NewDate(17, 5, 1999))
	dc := pricingMust(s.Actual365Fixed())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	quote := pricingMust(s.NewSimpleQuote(.06))
	rate := pricingMust(s.NewFlatForwardFromQuote(today, quote, dc))
	dividend := pricingMust(s.NewFlatForward(today, 0, dc))
	vol := pricingMust(s.BlackConstantVol(BlackConstantVolConfig{ReferenceDate: today, Volatility: .2, DayCounter: dc}))
	p := pricingMust(s.NewBlackScholesProcessFromCurves(36, rate, dividend, vol))
	dates := []Date{pricingMust(today.AddDays(180)), expiry}
	exercise := pricingMust(s.NewBermudanExercise(dates))
	option := pricingMust(s.NewBermudanOption(Put, 36, exercise, settings))
	engine := pricingMust(s.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), Hermite))
	pricingOK(t, option.SetMCAmericanEngine(engine))
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewBermudanOption(Put, 36, exercise, settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session Bermudan: %v", err)
	}
	if _, err := s.NewBermudanOption(Put, 36, nil, settings); err == nil {
		t.Fatal("nil exercise accepted")
	}
	for _, o := range []object{exercise.object, rate.object, dividend.object, vol.object, p.object, engine.object, settings.object} {
		pricingOK(t, o.Close())
	}
	dates[0] = expiry
	initial := pricingMust(option.NPV())
	pricingOK(t, quote.SetValue(.08))
	updated := pricingMust(option.NPV())
	freshSettings := pricingMust(s.NewSettings())
	pricingOK(t, freshSettings.SetEvaluationDate(today))
	freshExercise := pricingMust(s.NewBermudanExercise([]Date{pricingMust(today.AddDays(180)), expiry}))
	fresh := pricingMust(s.NewBermudanOption(Put, 36, freshExercise, freshSettings))
	freshP := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 36, RiskFreeRate: .08, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	freshEngine := pricingMust(s.NewMCAmericanEngineWithBasis(freshP, mcDepthConfig(), Hermite))
	if got := pricingMust(fresh.PriceMCAmerican(freshEngine)); math.IsNaN(updated) || math.IsInf(updated, 0) || updated == initial || got != updated {
		t.Fatalf("updated %g fresh %g initial %g", updated, got, initial)
	}
	pricingOK(t, quote.SetValue(.06))
	pricingOK(t, quote.Close())
	if got := pricingMust(option.NPV()); got != initial {
		t.Fatalf("restored %g initial %g", got, initial)
	}
	if _, err := s.NewBermudanOption(Put, 36, exercise, freshSettings); err == nil {
		t.Fatal("closed exercise accepted")
	}
}

func TestMCBermudanQuantLibOracles(t *testing.T) {
	var fixture struct {
		Bermudan struct {
			Dates          []int   `json:"dates"`
			FD             float64 `json:"fd"`
			ExerciseOnlyMC float64 `json:"exercise_only_mc"`
		} `json:"bermudan"`
	}
	pricingOK(t, json.Unmarshal(pricingMust(os.ReadFile("testdata/mc_american_depth.json")), &fixture))
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(17, 5, 1998))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(pricingMust(today.AddDays(-2))))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 36, RiskFreeRate: .06, Volatility: .2, ReferenceDate: today, DayCounter: pricingMust(s.Actual365Fixed())}))
	dates := make([]Date, len(fixture.Bermudan.Dates))
	for i, days := range fixture.Bermudan.Dates {
		dates[i] = pricingMust(today.AddDays(int64(days)))
	}
	exercise := pricingMust(s.NewBermudanExercise(dates))
	option := pricingMust(s.NewBermudanOption(Put, 40, exercise, settings))
	for _, steps := range []uint{3, 75} {
		cfg := mcDepthConfig()
		cfg.Steps, cfg.Samples = &steps, pricingPtr(uint(32768))
		cfg.PolynomialOrder, cfg.CalibrationSamples = pricingPtr(uint(2)), pricingPtr(uint(8192))
		value := pricingMust(option.PriceMCAmerican(pricingMust(s.NewMCAmericanEngineWithBasis(p, cfg, Monomial))))
		expected := fixture.Bermudan.FD
		if steps == 3 {
			expected = fixture.Bermudan.ExerciseOnlyMC
		}
		if math.IsNaN(value) || math.IsInf(value, 0) || math.Abs(value-expected) >= 2.34*mcDepthError(t, option) {
			t.Fatalf("steps %d value %.15g expected %.15g", steps, value, expected)
		}
	}
}

func TestMCAmericanBasisQuantLibOracles(t *testing.T) {
	var fixture struct {
		Bases []struct {
			Family PolynomialType `json:"family"`
			Value  float64        `json:"value"`
		} `json:"bases"`
	}
	pricingOK(t, json.Unmarshal(pricingMust(os.ReadFile("testdata/mc_american_depth.json")), &fixture))
	if len(fixture.Bases) != 5 {
		t.Fatal("expected five supported basis references")
	}
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(17, 5, 1998))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(pricingMust(today.AddDays(-2))))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 36, RiskFreeRate: .06, Volatility: .2, ReferenceDate: today, DayCounter: pricingMust(s.Actual365Fixed())}))
	option := pricingMust(s.NewAmericanOptionUntil(Put, 40, pricingMust(today.AddDays(365)), settings))
	cfg := mcDepthConfig()
	cfg.Steps, cfg.Samples, cfg.PolynomialOrder = pricingPtr(uint(12)), pricingPtr(uint(4096)), pricingPtr(uint(2))
	values := map[float64]bool{}
	for _, row := range fixture.Bases {
		engine := pricingMust(s.NewMCAmericanEngineWithBasis(p, cfg, row.Family))
		value := pricingMust(option.PriceMCAmerican(engine))
		if math.IsNaN(value) || math.IsInf(value, 0) || math.Abs(value-row.Value) >= 2.34*mcDepthError(t, option) {
			t.Fatalf("basis %v price %.15g expected %.15g", row.Family, value, row.Value)
		}
		values[value] = true
	}
	if len(values) != 5 {
		t.Fatal("basis families did not select distinct regressions")
	}
}

func TestMCChebyshevCallDomainRecovery(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(17, 5, 1998))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 36, RiskFreeRate: .06, Volatility: .2, ReferenceDate: today, DayCounter: pricingMust(s.Actual365Fixed())}))
	option := pricingMust(s.NewAmericanOptionUntil(Call, 30, pricingMust(today.AddDays(365)), settings))
	if _, err := option.PriceMCAmerican(pricingMust(s.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), Chebyshev2nd))); err == nil {
		t.Fatal("Chebyshev2nd call domain accepted")
	}
	value := pricingMust(option.PriceMCAmerican(pricingMust(s.NewMCAmericanEngineWithBasis(p, mcDepthConfig(), Laguerre))))
	if math.IsNaN(value) || math.IsInf(value, 0) {
		t.Fatal("valid basis failed to recover")
	}
}

func mcDepthError(t *testing.T, option *VanillaOption) float64 {
	t.Helper()
	value := pricingMust(option.ErrorEstimate())
	if math.IsNaN(value) || math.IsInf(value, 0) || value <= 0 {
		t.Fatalf("invalid standard error %g", value)
	}
	return value
}
