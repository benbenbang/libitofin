package itofin

import (
	"errors"
	"math"
	"testing"
)

func TestCashDigitalAnalytic(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	process := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	d2 := (math.Log(100./105.) + .03 - .02) / .2
	density := math.Exp(-d2*d2/2) / math.Sqrt(2*math.Pi)
	for _, kind := range []OptionType{Call, Put} {
		sign := 1.
		if kind == Put {
			sign = -1
		}
		o := pricingMust(s.NewCashOrNothingOption(kind, 105, 12, pricingMust(today.AddDays(360)), settings))
		if _, err := o.NPV(); err == nil {
			t.Fatal("missing engine accepted")
		}
		pricingOK(t, o.SetEngine(process))
		value := 12 * math.Exp(-.05) * math.Erfc(-sign*d2/math.Sqrt2) / 2
		delta := sign * 12 * math.Exp(-.05) * density / (100 * .2)
		gamma := -delta / 100 * (1 + d2/.2)
		want := []float64{value, delta, gamma, 100*delta - value, -100 * delta, -100 * delta * (d2 + .2), .05*value - .03*100*delta - .5*.2*.2*100*100*gamma}
		queries := []func() (float64, error){o.NPV, o.Delta, o.Gamma, o.Rho, o.DividendRho, o.Vega, o.Theta}
		for i, query := range queries {
			pricingNear(t, pricingMust(query()), want[i], 2e-12)
		}
	}
}

func TestCashDigitalDomains(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	settings := pricingMust(s.NewSettings())
	expiry := pricingMust(NewDate(15, 6, 2027))
	for _, bad := range [][2]float64{{-1, 1}, {math.NaN(), 1}, {math.Inf(1), 1}, {1, math.NaN()}, {1, math.Inf(1)}} {
		if _, err := s.NewCashOrNothingOption(Call, bad[0], bad[1], expiry, settings); err == nil {
			t.Fatal("invalid parameter accepted")
		}
	}
	if _, err := s.NewCashOrNothingOption(OptionType(2), 100, 1, expiry, settings); err == nil {
		t.Fatal("invalid option kind accepted")
	}
	if _, err := s.NewCashOrNothingOption(Call, 100, 1, Date{}, settings); err == nil {
		t.Fatal("invalid expiry accepted")
	}
	if _, err := s.NewCashOrNothingOption(Call, 100, 1, expiry, nil); err == nil {
		t.Fatal("nil settings accepted")
	}
	if _, err := other.NewCashOrNothingOption(Call, 100, 1, expiry, settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign settings: %v", err)
	}
	pricingOK(t, settings.Close())
	if _, err := s.NewCashOrNothingOption(Call, 100, 1, expiry, settings); err == nil {
		t.Fatal("closed settings accepted")
	}
}
