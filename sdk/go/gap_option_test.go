package itofin

import (
	"math"
	"testing"
)

func gapNear(t *testing.T, got, want, tolerance float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.IsNaN(want) || math.IsInf(want, 0) || math.Abs(got-want) > tolerance {
		t.Fatalf("gap value %.17g, want %.17g within %.4g", got, want, tolerance)
	}
}

func gapMarket(t *testing.T) (*Session, Date, *Settings, *BlackScholesProcess) {
	t.Helper()
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	return s, pricingMust(today.AddDays(360)), settings, p
}

func TestGapFactoryPricingAndUnsupportedAnalytic(t *testing.T) {
	s, expiry, settings, process := gapMarket(t)
	engine := pricingMust(s.NewFdBlackScholesVanillaEngine(process, FdConfig{TGrid: pricingPtr(uint(400)), XGrid: pricingPtr(uint(400)), DampingSteps: pricingPtr(uint(2))}))
	for _, kind := range []OptionType{Call, Put} {
		o := pricingMust(s.NewGapOption(kind, 100, 100, expiry, settings))
		if _, err := o.NPV(); err == nil {
			t.Fatal("missing engine accepted")
		}
		pricingOK(t, o.SetEngine(process))
		if _, err := o.NPV(); err == nil {
			t.Fatal("analytic Black engine accepted Gap")
		}
		vanilla := pricingMust(s.NewVanillaOption(kind, 100, expiry, settings))
		gapNear(t, pricingMust(o.PriceFd(engine)), pricingMust(vanilla.PriceFd(engine)), 1e-12)
		for _, query := range []func() (float64, error){o.NPV, o.Delta, o.Gamma, o.Theta} {
			value := pricingMust(query())
			if math.IsNaN(value) || math.IsInf(value, 0) {
				t.Fatal("nonfinite FD result")
			}
		}
		if _, err := o.Vega(); err == nil {
			t.Fatal("missing FD vega accepted")
		}
		pricingOK(t, o.Close())
		if _, err := o.NPV(); err == nil {
			t.Fatal("closed option accepted")
		}
	}
}
