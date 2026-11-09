package itofin

import (
	"errors"
	"math"
	"testing"
)

type dividendMarket struct {
	s        *Session
	settings *Settings
	process  *BlackScholesProcess
	today    Date
}

func newDividendMarket(t *testing.T, today Date, spot, r, q, vol float64) dividendMarket {
	t.Helper()
	s := pricingMust(NewSession())
	t.Cleanup(func() { s.Close() })
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{
		Spot: spot, RiskFreeRate: r, DividendYield: q, Volatility: vol,
		ReferenceDate: today, DayCounter: pricingMust(s.Actual360()),
	}))
	return dividendMarket{s, settings, p, today}
}

func (m dividendMarket) option(kind OptionType, strike float64, expiry Date, american bool) *VanillaOption {
	if american {
		return pricingMust(m.s.NewAmericanOption(kind, strike, m.today, expiry, m.settings))
	}
	return pricingMust(m.s.NewVanillaOption(kind, strike, expiry, m.settings))
}

var dividendGrid = FdConfig{TGrid: pricingPtr(uint(100)), XGrid: pricingPtr(uint(400))}

// REFERENCE SUBSTITUTION (#1369): FD Spot values cached from a locally built
// C++ QuantLib 9863b578a on the americanoption.cpp:731-761 fixture.
func TestFdCashDividendsReproduceCachedQuantLib(t *testing.T) {
	m := newDividendMarket(t, pricingMust(NewDate(27, 2, 2021)), 100, .04, .08, .3)
	maturity := pricingMust(NewDate(27, 2, 2022))
	first := pricingMust(NewDate(27, 12, 2021))
	greeks := func(e *FdBlackScholesVanillaEngine, kind OptionType, american bool) [3]float64 {
		o := m.option(kind, 100, maturity, american)
		pricingOK(t, o.SetFdEngine(e))
		return [3]float64{pricingMust(o.NPV()), pricingMust(o.Delta()), pricingMust(o.Gamma())}
	}
	single := pricingMust(m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid, []Date{first}, []float64{10}))
	two := pricingMust(m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid,
		[]Date{pricingMust(NewDate(27, 6, 2021)), first}, []float64{5, 10}))
	after := pricingMust(m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid,
		[]Date{first, pricingMust(NewDate(27, 4, 2022))}, []float64{10, 7}))
	for _, row := range []struct {
		engine   *FdBlackScholesVanillaEngine
		kind     OptionType
		american bool
		want     [3]float64
	}{
		{single, Call, true, [3]float64{9.3876035416433616, 0.50439288657051151, 0.014825610960493722}},
		{single, Put, false, [3]float64{19.549295792091659, -0.57167700814074907, 0.011801370341950067}},
		{two, Call, true, [3]float64{7.9057708867966641, 0.49147953278341305, 0.01871370359184878}},
		{two, Put, false, [3]float64{22.763590143100142, -0.62808693189317422, 0.011427611391172732}},
	} {
		got := greeks(row.engine, row.kind, row.american)
		for i := range got {
			pricingNear(t, got[i], row.want[i], 1e-8)
		}
		if row.engine == single && greeks(after, row.kind, row.american) != got {
			t.Fatal("a dividend after maturity changed the price")
		}
	}
}

// testFdDividendAtTZero (dividendoption.cpp:856-919), Spot arm.
func TestFdCashDividendTodayMatchesAnalyticAndHasNoTheta(t *testing.T) {
	today := pricingMust(NewDate(27, 2, 2005))
	expiry := pricingMust(NewDate(13, 4, 2005))
	m := newDividendMarket(t, today, 54.625, 0, 0, .282922)
	dates, amounts := []Date{today}, []float64{1}
	fd := pricingMust(m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process,
		FdConfig{TGrid: pricingPtr(uint(50)), XGrid: pricingPtr(uint(400))}, dates, amounts))
	analytic := pricingMust(m.s.NewAnalyticDividendEuropeanEngine(m.process, dates, amounts))
	expected := pricingMust(m.option(Call, 55, expiry, false).PriceAnalyticDividend(analytic))
	for _, american := range []bool{false, true} {
		o := m.option(Call, 55, expiry, american)
		pricingNear(t, pricingMust(o.PriceFd(fd)), expected, 1e-4)
		if _, err := o.Theta(); err == nil {
			t.Fatal("theta with a dividend today succeeded")
		}
	}
}

// testEuropeanKnownValue (dividendoption.cpp:166-221): Hull 12.8.
func TestAnalyticDividendEuropeanKnownValue(t *testing.T) {
	today := pricingMust(NewDate(27, 2, 2021))
	m := newDividendMarket(t, today, 40, .09, 0, .3)
	engine := pricingMust(m.s.NewAnalyticDividendEuropeanEngine(m.process,
		[]Date{pricingMust(today.AddDays(60)), pricingMust(today.AddDays(150))}, []float64{.5, .5}))
	expiry := pricingMust(today.AddDays(180))
	pricingNear(t, pricingMust(m.option(Call, 40, expiry, false).PriceAnalyticDividend(engine)), 3.67, 1e-2)
	if _, err := m.option(Call, 40, expiry, true).PriceAnalyticDividend(engine); err == nil {
		t.Fatal("analytic dividend engine priced an American option")
	}
}

func TestCashDividendEnginesErrorsAndLifetime(t *testing.T) {
	m := newDividendMarket(t, pricingMust(NewDate(27, 2, 2021)), 100, .04, .08, .3)
	other := pricingMust(NewSession())
	defer other.Close()
	maturity := pricingMust(NewDate(27, 2, 2022))
	dates, amounts := []Date{pricingMust(NewDate(27, 12, 2021))}, []float64{10}
	if _, err := m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid, dates, []float64{10, 5}); err == nil {
		t.Fatal("FD accepted mismatched dividend lengths")
	}
	if _, err := m.s.NewAnalyticDividendEuropeanEngine(m.process, dates, nil); err == nil {
		t.Fatal("analytic accepted mismatched dividend lengths")
	}
	if _, err := m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid, []Date{{}}, amounts); err == nil {
		t.Fatal("FD accepted an invalid dividend date")
	}
	if _, err := m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, FdConfig{XGrid: pricingPtr(uint(2))}, dates, amounts); err == nil {
		t.Fatal("FD accepted an invalid grid")
	}
	if _, err := m.s.NewAnalyticDividendEuropeanEngine(nil, dates, amounts); err == nil {
		t.Fatal("nil process accepted")
	}
	if _, err := other.NewAnalyticDividendEuropeanEngine(m.process, dates, amounts); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session process: %v", err)
	}
	if _, err := other.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid, dates, amounts); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session process: %v", err)
	}
	if err := m.option(Put, 100, maturity, false).SetAnalyticDividendEngine(nil); err == nil {
		t.Fatal("nil engine accepted")
	}
	plain := pricingMust(m.s.NewFdBlackScholesVanillaEngine(m.process, dividendGrid))
	empty := pricingMust(m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid, nil, nil))
	fd := pricingMust(m.s.NewFdBlackScholesVanillaEngineWithCashDividends(m.process, dividendGrid, dates, amounts))
	analytic := pricingMust(m.s.NewAnalyticDividendEuropeanEngine(m.process, dates, amounts))
	pricingOK(t, m.process.Close())
	for _, american := range []bool{false, true} {
		o := m.option(Put, 100, maturity, american)
		want := pricingMust(o.PriceFd(plain))
		if got := pricingMust(o.PriceFd(empty)); math.Float64bits(got) != math.Float64bits(want) {
			t.Fatalf("empty schedule %v differs from the plain engine %v", got, want)
		}
		if !american {
			pricingNear(t, pricingMust(o.PriceFd(fd)), 19.549295792091659, 1e-8)
		}
	}
	o := m.option(Put, 100, maturity, false)
	pricingOK(t, o.SetAnalyticDividendEngine(analytic))
	if got := pricingMust(o.NPV()); math.IsNaN(got) || math.IsInf(got, 0) {
		t.Fatalf("analytic engine lost its process: %v", got)
	}
}
