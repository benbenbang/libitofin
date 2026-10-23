package itofin

import (
	"math"
	"sync"
	"testing"
)

func TestCashDigitalOriginalCoreZeroVolatilityLadder(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	for _, kind := range []OptionType{Call, Put} {
		sign := 1.
		if kind == Put {
			sign = -1
		}
		for _, cash := range []float64{-12, 0, 12} {
			for _, volatility := range []float64{0, 1e-18} {
				for _, spot := range []float64{90, 100, 110} {
					process := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: spot, Volatility: volatility, ReferenceDate: today, DayCounter: dc}))
					o := pricingMust(s.NewCashOrNothingOption(kind, 100, cash, pricingMust(today.AddDays(360)), settings))
					pricingOK(t, o.SetEngine(process))
					probability := 0.
					if spot == 100 {
						probability = .5
					} else if sign*(spot-100) > 0 {
						probability = 1
					}
					delta := sign * probability
					wants := []float64{cash * probability, delta, 0, 0, spot*delta - cash*probability, -spot * delta, 0}
					queries := []func() (float64, error){o.NPV, o.Delta, o.Gamma, o.Vega, o.Rho, o.DividendRho, o.Theta}
					for i, query := range queries {
						pricingNear(t, pricingMust(query()), wants[i], 0)
					}
					pricingOK(t, o.Close())
					pricingOK(t, process.Close())
				}
			}
		}
	}
}

func TestCashDigitalLiveInputsRetainedAndConcurrentPricing(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	quotes := []*SimpleQuote{pricingMust(s.NewSimpleQuote(.05)), pricingMust(s.NewSimpleQuote(.02)), pricingMust(s.NewSimpleQuote(.2))}
	risk := pricingMust(s.NewFlatForwardFromQuote(today, quotes[0], dc))
	dividend := pricingMust(s.NewFlatForwardFromQuote(today, quotes[1], dc))
	vol := pricingMust(s.NewBlackConstantVolFromQuote(today, quotes[2], dc, nil))
	process := pricingMust(s.NewBlackScholesProcessFromCurves(100, risk, dividend, vol))
	expiry := pricingMust(today.AddDays(360))
	o := pricingMust(s.NewCashOrNothingOption(Call, 105, 12, expiry, settings))
	pricingOK(t, o.SetEngine(process))
	before := pricingMust(o.NPV())
	snapshot := pricingMust(o.Results())
	for _, close := range []func() error{risk.Close, dividend.Close, vol.Close, dc.Close} {
		pricingOK(t, close())
	}
	values := []float64{.05, .02, .2}
	for i, value := range []float64{.07, .04, .3} {
		pricingOK(t, quotes[i].SetValue(value))
		values[i] = value
		if pricingMust(o.IsCalculated()) {
			t.Fatal("quote change did not invalidate")
		}
		d2 := (math.Log(100./105.) + values[0] - values[1] - values[2]*values[2]/2) / values[2]
		want := 12 * math.Exp(-values[0]) * math.Erfc(-d2/math.Sqrt2) / 2
		pricingNear(t, pricingMust(o.NPV()), want, 2e-12)
	}
	if snapshot.NPV == nil || *snapshot.NPV != before {
		t.Fatal("result snapshot changed")
	}
	want := pricingMust(o.NPV())
	var wg sync.WaitGroup
	for i := 0; i < 12; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			v, err := o.Price(process)
			if err != nil || v != want {
				t.Errorf("concurrent price %g %v", v, err)
			}
		}()
	}
	wg.Wait()
	pricingOK(t, process.Close())
	for _, quote := range quotes {
		pricingOK(t, quote.Close())
	}
	pricingOK(t, settings.SetEvaluationDate(expiry))
	pricingNear(t, pricingMust(o.NPV()), 0, 0)
	pricingNear(t, pricingMust(o.Delta()), 0, 0)
	pricingOK(t, settings.SetEvaluationDate(today))
	pricingOK(t, settings.Close())
	pricingNear(t, pricingMust(o.NPV()), want, 2e-12)
	pricingOK(t, o.Close())
	pricingOK(t, o.Close())
	if _, err := o.NPV(); err == nil {
		t.Fatal("released option accepted")
	}
}

func TestCashDigitalUnsupportedHestonAndRecovery(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	heston := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .05, DividendYield: .02, Spot: 100, V0: .04, Kappa: 1, Theta: .04, Sigma: .3, Rho: -.5, ReferenceDate: today, DayCounter: dc}))
	model := pricingMust(s.NewHestonModel(heston))
	o := pricingMust(s.NewCashOrNothingOption(Put, 100, 12, pricingMust(today.AddDays(360)), settings))
	if _, err := o.PriceHeston(model, 64); err == nil {
		t.Fatal("plain-payoff-only engine accepted digital")
	}
	process := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	if pricingMust(o.Price(process)) <= 0 {
		t.Fatal("analytic engine did not recover")
	}
}
