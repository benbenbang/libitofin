package itofin

import (
	"errors"
	"math"
	"sync"
	"testing"
)

func TestAssetDigitalLiveInputsRetainedAndConcurrent(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 6, 2026))
	expiry := pricingMust(today.AddDays(360))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	quotes := []*SimpleQuote{pricingMust(s.NewSimpleQuote(.05)), pricingMust(s.NewSimpleQuote(.02)), pricingMust(s.NewSimpleQuote(.2))}
	risk := pricingMust(s.NewFlatForwardFromQuote(today, quotes[0], dc))
	dividend := pricingMust(s.NewFlatForwardFromQuote(today, quotes[1], dc))
	vol := pricingMust(s.NewBlackConstantVolFromQuote(today, quotes[2], dc, nil))
	process := pricingMust(s.NewBlackScholesProcessFromCurves(100, risk, dividend, vol))
	engine := pricingMust(s.NewFdBlackScholesVanillaEngine(process, FdConfig{}))
	option := pricingMust(s.NewAssetOrNothingOption(Call, 105, expiry, settings))
	before := pricingMust(option.PriceFd(engine))
	if math.IsNaN(before) || math.IsInf(before, 0) {
		t.Fatalf("initial FD value is nonfinite: %g", before)
	}
	snapshot := pricingMust(option.Results())
	for _, close := range []func() error{risk.Close, dividend.Close, vol.Close, process.Close} {
		pricingOK(t, close())
	}
	values := []float64{.05, .02, .2}
	previous := before
	for index, value := range []float64{.07, .04, .3} {
		pricingOK(t, quotes[index].SetValue(value))
		values[index] = value
		if pricingMust(option.IsCalculated()) {
			t.Fatal("live input did not invalidate")
		}
		freshProcess := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: values[0], DividendYield: values[1], Volatility: values[2], ReferenceDate: today, DayCounter: dc}))
		freshEngine := pricingMust(s.NewFdBlackScholesVanillaEngine(freshProcess, FdConfig{}))
		fresh := pricingMust(s.NewAssetOrNothingOption(Call, 105, expiry, settings))
		want := pricingMust(fresh.PriceFd(freshEngine))
		if math.IsNaN(want) || math.IsInf(want, 0) {
			t.Fatalf("fresh FD reference is nonfinite: %g", want)
		}
		got := pricingMust(option.NPV())
		if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > 1e-10 || math.Abs(got-previous) < 1e-4 {
			t.Fatalf("live FD value %g, fresh %g, previous %g", got, want, previous)
		}
		previous = got
		for _, close := range []func() error{fresh.Close, freshEngine.Close, freshProcess.Close} {
			pricingOK(t, close())
		}
	}
	if snapshot.NPV == nil || *snapshot.NPV != before {
		t.Fatal("frozen snapshot changed")
	}
	var group sync.WaitGroup
	for index := 0; index < 8; index++ {
		group.Add(1)
		go func() {
			defer group.Done()
			got, err := option.PriceFd(engine)
			if err != nil || math.IsNaN(got) || math.IsInf(got, 0) || math.Float64bits(got) != math.Float64bits(previous) {
				t.Errorf("concurrent FD value %g, %v", got, err)
			}
		}()
	}
	group.Wait()
	pricingOK(t, engine.Close())
	for _, quote := range quotes {
		pricingOK(t, quote.Close())
	}
	pricingOK(t, dc.Close())
	pricingOK(t, settings.SetEvaluationDate(expiry))
	if pricingMust(option.NPV()) != 0 {
		t.Fatal("expired option did not become zero")
	}
	pricingOK(t, settings.SetEvaluationDate(today))
	pricingOK(t, settings.Close())
	got := pricingMust(option.NPV())
	if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-previous) > 1e-10 {
		t.Fatal("released inputs were not retained")
	}
	pricingOK(t, option.Close())
	if _, err := option.NPV(); err == nil {
		t.Fatal("closed option accepted")
	}
}

func TestAssetDigitalEngineErrorsAndSessionBoundaries(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	process := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: today, DayCounter: dc}))
	option := pricingMust(s.NewAssetOrNothingOption(Put, 100, pricingMust(today.AddDays(360)), settings))
	if _, err := option.Price(process); err == nil {
		t.Fatal("unsupported analytic Black payoff accepted")
	}
	mc := pricingMust(s.NewMCEuropeanEngine(process, MCConfig{Steps: pricingPtr(uint(1)), Samples: pricingPtr(uint(100))}))
	if _, err := option.PriceMC(mc); err == nil {
		t.Fatal("plain-only MC payoff accepted")
	}
	fd := pricingMust(s.NewFdBlackScholesVanillaEngine(process, FdConfig{}))
	want := pricingMust(option.PriceFd(fd))
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	otherDC := pricingMust(other.Actual360())
	otherProcess := pricingMust(other.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, Volatility: .2, ReferenceDate: today, DayCounter: otherDC}))
	foreign := pricingMust(other.NewFdBlackScholesVanillaEngine(otherProcess, FdConfig{}))
	if _, err := option.PriceFd(foreign); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign engine: %v", err)
	}
	if _, err := option.PriceFd(nil); err == nil {
		t.Fatal("nil engine accepted")
	}
	if got := pricingMust(option.NPV()); math.IsNaN(got) || math.IsInf(got, 0) || got != want {
		t.Fatal("rejected attachment changed existing FD state")
	}
	pricingOK(t, fd.Close())
	if _, err := option.PriceFd(fd); err == nil {
		t.Fatal("closed engine accepted")
	}
	for _, kind := range []OptionType{Call, Put} {
		flat := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, ReferenceDate: today, DayCounter: dc}))
		flatEngine := pricingMust(s.NewFdBlackScholesVanillaEngine(flat, FdConfig{}))
		digital := pricingMust(s.NewAssetOrNothingOption(kind, 100, pricingMust(today.AddDays(360)), settings))
		if _, err := digital.PriceFd(flatEngine); err == nil {
			t.Fatal("degenerate zero-vol FD mesh did not return native error")
		}
	}
	pricingOK(t, s.Close())
	if _, err := option.NPV(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
}
