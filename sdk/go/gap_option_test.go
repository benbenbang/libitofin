package itofin

import (
	"errors"
	"math"
	"sync"
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

func TestGapSignedPaymentsAndRetainedGraph(t *testing.T) {
	s, expiry, settings, process := gapMarket(t)
	engine := pricingMust(s.NewFdBlackScholesVanillaEngine(process, FdConfig{TGrid: pricingPtr(uint(400)), XGrid: pricingPtr(uint(400)), DampingSteps: pricingPtr(uint(2))}))
	call := pricingMust(s.NewGapOption(Call, 100, 160, expiry, settings))
	put := pricingMust(s.NewGapOption(Put, 100, 40, expiry, settings))
	for _, o := range []*VanillaOption{call, put} {
		value := pricingMust(o.PriceFd(engine))
		if math.IsNaN(value) || math.IsInf(value, 0) || value >= 0 {
			t.Fatalf("signed negative payoff was lost: %v", value)
		}
	}
	want := pricingMust(call.NPV())
	pricingOK(t, engine.Close())
	pricingOK(t, process.Close())
	pricingOK(t, settings.Close())
	gapNear(t, pricingMust(call.NPV()), want, 0)
	var workers sync.WaitGroup
	for range 4 {
		workers.Go(func() {
			value, err := call.NPV()
			if err != nil || math.IsNaN(value) || value != want {
				t.Errorf("serialized retained graph query: value=%v error=%v", value, err)
			}
		})
	}
	workers.Wait()
	pricingOK(t, s.Close())
	if _, err := call.NPV(); err == nil {
		t.Fatal("closed session accepted")
	}
}

func TestGapFactoryDomainsAndSessions(t *testing.T) {
	s, expiry, settings, _ := gapMarket(t)
	for _, row := range [][2]float64{{-1, 100}, {math.NaN(), 100}, {math.Inf(1), 100}, {100, math.NaN()}, {100, math.Inf(1)}, {100, math.Inf(-1)}} {
		if _, err := s.NewGapOption(Call, row[0], row[1], expiry, settings); err == nil {
			t.Fatal("invalid gap parameter accepted")
		}
	}
	for _, row := range [][2]float64{{0, -1}, {100, 0}, {100, -20}} {
		pricingMust(s.NewGapOption(Call, row[0], row[1], expiry, settings))
	}
	if _, err := s.NewGapOption(OptionType(2), 100, 100, expiry, settings); err == nil {
		t.Fatal("invalid kind accepted")
	}
	if _, err := s.NewGapOption(Call, 100, 100, Date{}, settings); err == nil {
		t.Fatal("invalid expiry accepted")
	}
	if _, err := s.NewGapOption(Call, 100, 100, expiry, nil); err == nil {
		t.Fatal("nil settings accepted")
	}
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	if _, err := other.NewGapOption(Call, 100, 100, expiry, settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign settings: %v", err)
	}
	pricingOK(t, settings.Close())
	if _, err := s.NewGapOption(Call, 100, 100, expiry, settings); err == nil {
		t.Fatal("closed settings accepted")
	}
}

func TestGapLiveCurvesAndMixedEngineSessions(t *testing.T) {
	s, expiry, settings, _ := gapMarket(t)
	today := pricingMust(expiry.AddDays(-360))
	dc := pricingMust(s.Actual360())
	rate := pricingMust(s.NewSimpleQuote(.05))
	dividend := pricingMust(s.NewSimpleQuote(.02))
	volatility := pricingMust(s.NewSimpleQuote(.2))
	risk := pricingMust(s.NewFlatForwardFromQuote(today, rate, dc))
	div := pricingMust(s.NewFlatForwardFromQuote(today, dividend, dc))
	vol := pricingMust(s.NewBlackConstantVolFromQuote(today, volatility, dc, nil))
	process := pricingMust(s.NewBlackScholesProcessFromCurves(100, risk, div, vol))
	cfg := FdConfig{TGrid: pricingPtr(uint(400)), XGrid: pricingPtr(uint(400)), DampingSteps: pricingPtr(uint(2))}
	engine := pricingMust(s.NewFdBlackScholesVanillaEngine(process, cfg))
	o := pricingMust(s.NewGapOption(Call, 105, 120, expiry, settings))
	before := pricingMust(o.PriceFd(engine))
	if math.IsNaN(before) || math.IsInf(before, 0) {
		t.Fatal("nonfinite initial gap price")
	}
	for _, close := range []func() error{engine.Close, process.Close, risk.Close, div.Close, vol.Close, dc.Close} {
		pricingOK(t, close())
	}
	for _, row := range []struct {
		quote *SimpleQuote
		value float64
	}{{rate, .07}, {dividend, .04}, {volatility, .3}} {
		pricingOK(t, row.quote.SetValue(row.value))
		if pricingMust(o.IsCalculated()) {
			t.Fatal("live quote update did not invalidate gap")
		}
		after := pricingMust(o.NPV())
		if math.IsNaN(after) || math.IsInf(after, 0) || math.Abs(after-before) <= 1e-8 {
			t.Fatal("live quote update not reflected")
		}
		before = after
	}
	freshDC := pricingMust(s.Actual360())
	freshProcess := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .07, DividendYield: .04, Volatility: .3, ReferenceDate: today, DayCounter: freshDC}))
	freshEngine := pricingMust(s.NewFdBlackScholesVanillaEngine(freshProcess, cfg))
	fresh := pricingMust(s.NewGapOption(Call, 105, 120, expiry, settings))
	gapNear(t, pricingMust(o.NPV()), pricingMust(fresh.PriceFd(freshEngine)), 1e-12)
	pricingOK(t, settings.SetEvaluationDate(expiry))
	gapNear(t, pricingMust(o.NPV()), 0, 0)
	pricingOK(t, settings.SetEvaluationDate(today))
	gapNear(t, pricingMust(o.NPV()), before, 1e-12)
	if err := o.SetFdEngine(nil); err == nil {
		t.Fatal("nil FD engine accepted")
	}
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	otherDC := pricingMust(other.Actual360())
	otherProcess := pricingMust(other.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, Volatility: .2, ReferenceDate: today, DayCounter: otherDC}))
	foreign := pricingMust(other.NewFdBlackScholesVanillaEngine(otherProcess, FdConfig{}))
	if err := o.SetFdEngine(foreign); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign engine: %v", err)
	}
	pricingOK(t, freshEngine.Close())
	if err := o.SetFdEngine(freshEngine); err == nil {
		t.Fatal("closed engine accepted")
	}
}
