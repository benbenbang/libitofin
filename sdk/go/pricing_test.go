package itofin

import (
	"errors"
	"math"
	"sync"
	"testing"
)

func pricingMust[T any](v T, err error) T {
	if err != nil {
		panic(err)
	}
	return v
}
func pricingOK(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}
func pricingNear(t *testing.T, got, want, tol float64) {
	t.Helper()
	if math.IsNaN(got) || math.Abs(got-want) > tol {
		t.Fatalf("got %.15g want %.15g tolerance %g", got, want, tol)
	}
}
func pricingPtr[T any](v T) *T { return &v }

// QuantLib testAnalyticVsCached, shared with the Python Heston pricing oracle.
func TestHestonCachedPriceAndLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(27, 12, 2004))
	expiry := pricingMust(NewDate(28, 3, 2005))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(ref))
	dc := pricingMust(s.ActualActualISDA())
	proc := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .0225, DividendYield: .02, Spot: 1, V0: .1, Kappa: 3.16, Theta: .09, Sigma: .4, Rho: -.2, ReferenceDate: ref, DayCounter: dc}))
	model := pricingMust(s.NewHestonModel(proc))
	opt := pricingMust(s.NewVanillaOption(Call, 1.05, expiry, settings))
	if _, err := opt.NPV(); err == nil {
		t.Fatal("missing engine accepted")
	}
	pricingOK(t, opt.SetHestonEngine(model, 64))
	pricingNear(t, pricingMust(opt.NPV()), .0404774515, 1e-8)
	pricingNear(t, pricingMust(opt.PriceHeston(model, 64)), .0404774515, 1e-8)
	for _, f := range []func() (float64, error){opt.Delta, opt.Gamma, opt.Theta, opt.Vega, opt.Rho, opt.DividendRho, opt.ErrorEstimate, opt.ExerciseProbability} {
		if _, err := f(); err == nil {
			t.Fatal("missing result accepted")
		}
	}
	if !pricingMust(opt.IsCalculated()) {
		t.Fatal("cache not valid")
	}
	pricingOK(t, opt.Calculate())
	if _, err := opt.Results(); err != nil {
		t.Fatal(err)
	}
	if err := opt.SetHestonEngine(model, 193); err == nil {
		t.Fatal("invalid integration order accepted")
	}
	// Releasing source handles must not destroy the instrument's retained model.
	pricingOK(t, proc.Close())
	pricingOK(t, model.Close())
	pricingOK(t, dc.Close())
	pricingNear(t, pricingMust(opt.NPV()), .0404774515, 1e-8)
	pricingOK(t, settings.SetEvaluationDate(pricingMust(expiry.AddDays(1))))
	if pricingMust(opt.IsCalculated()) {
		t.Fatal("date change did not invalidate cache")
	}
	pricingNear(t, pricingMust(opt.NPV()), 0, 0)
	pricingOK(t, opt.Close())
	pricingOK(t, opt.Close())
	if _, err := opt.NPV(); err == nil {
		t.Fatal("released option accepted")
	}
}

func TestOptionSessionIsolationAndConcurrentPricing(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	other := pricingMust(NewSession())
	defer other.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	expiry := pricingMust(today.AddDays(360))
	dc := pricingMust(s.Actual360())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	option := pricingMust(s.NewVanillaOption(Call, 100, expiry, settings))
	if _, err := other.NewVanillaOption(Call, 100, expiry, settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign settings: %v", err)
	}
	value := pricingMust(option.Price(p))
	var wg sync.WaitGroup
	errs := make(chan error, 16)
	for i := 0; i < 16; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			got, err := option.Price(p)
			if err != nil {
				errs <- err
			} else if got != value {
				errs <- errors.New("concurrent price differed")
			}
		}()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		t.Error(err)
	}
	for _, f := range []func() (float64, error){option.Delta, option.Gamma, option.Theta, option.Vega, option.Rho, option.DividendRho} {
		if v, err := f(); err != nil || math.IsNaN(v) {
			t.Fatalf("Greek: %v %v", v, err)
		}
	}
	if _, err := s.NewAmericanOption(Put, 100, expiry, today, settings); err == nil {
		t.Fatal("reversed exercise dates accepted")
	}
	if _, err := s.NewVanillaOption(OptionType(99), 100, expiry, settings); err == nil {
		t.Fatal("unknown option type accepted")
	}
}

// Same seed and three-standard-error band as test-suite/europeanoption.cpp.
func TestMCEuropeanOracleAndValidation(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	expiry := pricingMust(today.AddDays(360))
	dc := pricingMust(s.Actual360())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	cfg := MCConfig{Steps: pricingPtr(uint(1)), Samples: pricingPtr(uint(40000)), Seed: pricingPtr(uint32(42))}
	for _, strike := range []float64{90, 100, 110} {
		opt := pricingMust(s.NewVanillaOption(Call, strike, expiry, settings))
		analytic := pricingMust(opt.Price(p))
		engine := pricingMust(s.NewMCEuropeanEngine(p, cfg))
		pricingOK(t, opt.SetMCEngine(engine))
		mc := pricingMust(opt.PriceMC(engine))
		se := pricingMust(opt.ErrorEstimate())
		if se <= 0 || math.Abs(mc-analytic) >= 3*se {
			t.Fatalf("MC %g analytic %g SE %g", mc, analytic, se)
		}
		again := pricingMust(s.NewMCEuropeanEngine(p, cfg))
		if got := pricingMust(opt.PriceMC(again)); got != mc {
			t.Fatal("seed not repeatable")
		}
	}
	bad := cfg
	bad.Steps = nil
	if _, err := s.NewMCEuropeanEngine(p, bad); err == nil {
		t.Fatal("missing steps accepted")
	}
	bad = cfg
	bad.StepsPerYear = pricingPtr(uint(12))
	if _, err := s.NewMCEuropeanEngine(p, bad); err == nil {
		t.Fatal("duplicate steps accepted")
	}
	bad = cfg
	bad.AbsoluteTolerance = pricingPtr(.05)
	if _, err := s.NewMCEuropeanEngine(p, bad); err == nil {
		t.Fatal("duplicate sampling accepted")
	}
}

func TestMCHestonCachedOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(27, 12, 2004))
	expiry := pricingMust(NewDate(28, 3, 2005))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(ref))
	dc := pricingMust(s.ActualActualISDA())
	p := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .7, DividendYield: .4, Spot: 1.05, V0: .3, Kappa: 1.16, Theta: .2, Sigma: .8, Rho: .8, ReferenceDate: ref, DayCounter: dc}))
	cfg := MCConfig{StepsPerYear: pricingPtr(uint(11)), Samples: pricingPtr(uint(50000)), Seed: pricingPtr(uint32(1234)), Antithetic: pricingPtr(true)}
	engine := pricingMust(s.NewMCEuropeanHestonEngine(p, cfg))
	opt := pricingMust(s.NewVanillaOption(Put, 1.05, expiry, settings))
	pricingOK(t, opt.SetMCHestonEngine(engine))
	value := pricingMust(opt.PriceMCHeston(engine))
	se := pricingMust(opt.ErrorEstimate())
	if se <= 0 || se > 7.5e-4 || math.Abs(value-.0632851308977151) > 2.34*se {
		t.Fatalf("value %g SE %g", value, se)
	}
}

func TestMCAmericanCachedOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 5, 1998))
	settlement := pricingMust(NewDate(17, 5, 1998))
	expiry := pricingMust(NewDate(17, 5, 1999))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual365Fixed())
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 36, RiskFreeRate: .06, DividendYield: 0, Volatility: .2, ReferenceDate: settlement, DayCounter: dc}))
	cfg := MCConfig{Steps: pricingPtr(uint(75)), AbsoluteTolerance: pricingPtr(.02), Seed: pricingPtr(uint32(42)), Antithetic: pricingPtr(true), PolynomialOrder: pricingPtr(uint(3))}
	engine := pricingMust(s.NewMCAmericanEngine(p, cfg))
	opt := pricingMust(s.NewAmericanOption(Put, 36, settlement, expiry, settings))
	pricingOK(t, opt.SetMCAmericanEngine(engine))
	value := pricingMust(opt.PriceMCAmerican(engine))
	se := pricingMust(opt.ErrorEstimate())
	probability := pricingMust(opt.ExerciseProbability())
	if se <= 0 || math.Abs(value-2.054422273006143) >= 2.34*se {
		t.Fatalf("value %g SE %g", value, se)
	}
	pricingNear(t, probability, .48013, .015)
	euro := pricingMust(s.NewVanillaOption(Put, 36, expiry, settings))
	if value <= pricingMust(euro.Price(p)) {
		t.Fatal("American put lacks exercise premium")
	}
	if _, err := euro.PriceMCAmerican(engine); err == nil {
		t.Fatal("American engine accepted European exercise")
	}
}
