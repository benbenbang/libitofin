package itofin

import (
	"errors"
	"math"
	"strings"
	"sync"
	"testing"
)

func amortFinite(value float64) bool { return !math.IsNaN(value) && !math.IsInf(value, 0) }
func amortNear(t *testing.T, actual, expected float64) {
	t.Helper()
	if math.IsNaN(actual) || math.IsInf(actual, 0) || math.IsNaN(expected) || math.IsInf(expected, 0) || math.Abs(actual-expected) > 3e-11 {
		t.Fatalf("%g != %g", actual, expected)
	}
}
func amortMarket(t *testing.T, s *Session, limited bool) (AmortizingFloatingRateBondConfig, *SimpleQuote, *YieldTermStructure) {
	t.Helper()
	settings, err := s.NewSettings()
	bondCheck(t, err)
	today := testDate(t, 28, 9, 2026)
	bondCheck(t, settings.SetEvaluationDate(today))
	calendar, err := s.NullCalendar()
	bondCheck(t, err)
	maturity := calendar
	if limited {
		maturity, err = s.China()
		bondCheck(t, err)
	}
	dc, err := s.Actual360()
	bondCheck(t, err)
	currency, err := s.EUR()
	bondCheck(t, err)
	quote, err := s.NewSimpleQuote(0.04)
	bondCheck(t, err)
	forward, err := s.NewFlatForwardFromQuote(today, quote, dc)
	bondCheck(t, err)
	discount, err := s.NewFlatForward(today, 0.025, dc)
	bondCheck(t, err)
	index, err := s.NewCustomIborIndex(IborIndexConfig{FamilyName: "AmortizingGo", Tenor: Period{6, Months}, SettlementDays: 2, Currency: currency, FixingCalendar: calendar, ValueCalendar: calendar, MaturityCalendar: maturity, Convention: Following, DayCounter: dc, Forwarding: forward, Settings: settings})
	bondCheck(t, err)
	schedule, err := s.NewSchedule(ScheduleConfig{Start: testDate(t, 5, 10, 2026), End: testDate(t, 5, 10, 2027), Frequency: Quarterly, Calendar: calendar, Convention: Unadjusted})
	bondCheck(t, err)
	return AmortizingFloatingRateBondConfig{Notionals: []float64{1000, 800, 800, 500}, Schedule: schedule, Index: index, AccrualDayCounter: dc, Settings: settings}, quote, discount
}

func TestAmortizingFloatingPrincipalBalancesAndLiveGraph(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	config, quote, discount := amortMarket(t, s, false)
	bond, err := s.NewAmortizingFloatingRateBond(config)
	bondCheck(t, err)
	pricer, err := s.NewBlackIborCouponPricer(nil)
	bondCheck(t, err)
	bondCheck(t, bond.SetCouponPricer(pricer))
	bondCheck(t, pricer.Close())
	config.Notionals[0] = 1
	engine, err := s.NewDiscountingBondEngine(discount, config.Settings, nil)
	bondCheck(t, err)
	bondCheck(t, bond.SetEngine(engine))
	for _, close := range []func() error{config.Schedule.Close, config.Index.Close, config.AccrualDayCounter.Close, engine.Close, discount.Close} {
		bondCheck(t, close())
	}
	for _, row := range []struct {
		day, month int
		expected   float64
	}{{4, 1, 1000}, {5, 1, 800}, {5, 7, 500}, {5, 10, 0}} {
		date := testDate(t, row.day, row.month, 2027)
		actual, err := bond.Notional(&date)
		bondCheck(t, err)
		amortNear(t, actual, row.expected)
	}
	initial, err := bond.NPV()
	bondCheck(t, err)
	bondCheck(t, quote.SetValue(0.06))
	updated, err := bond.NPV()
	bondCheck(t, err)
	if !amortFinite(initial) || !amortFinite(updated) || updated <= initial {
		t.Fatal("live forwarding graph did not update")
	}
	bondCheck(t, bond.Freeze())
	bondCheck(t, quote.SetValue(0.03))
	frozen, err := bond.NPV()
	bondCheck(t, err)
	amortNear(t, frozen, updated)
	bondCheck(t, bond.Unfreeze())
	var wait sync.WaitGroup
	for range 8 {
		wait.Add(1)
		go func() {
			defer wait.Done()
			value, err := bond.NPV()
			if err != nil || !amortFinite(value) || value >= updated {
				t.Errorf("concurrent retained graph: %g %v", value, err)
			}
		}()
	}
	wait.Wait()
}

func TestAmortizingFloatingRetainedPrincipalGuardGeometry(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	config, _, _ := amortMarket(t, s, true)
	bond, err := s.NewAmortizingFloatingRateBond(config)
	bondCheck(t, err)
	leg, err := bond.Cashflows()
	bondCheck(t, err)
	index, err := bond.Index()
	bondCheck(t, err)
	for _, close := range []func() error{bond.Close, config.Index.Close, config.Schedule.Close, config.AccrualDayCounter.Close} {
		bondCheck(t, close())
	}
	for i, position := range []int{1, 4, 6} {
		flow, err := leg.At(position)
		bondCheck(t, err)
		amount, err := flow.Amount()
		bondCheck(t, err)
		amortNear(t, amount, []float64{200, 300, 500}[i])
		bondCheck(t, flow.Close())
	}
	for _, position := range []int{0, 2, 3, 5} {
		flow, err := leg.At(position)
		bondCheck(t, err)
		if _, err := flow.Amount(); err == nil || !strings.Contains(err.Error(), "2026") {
			t.Fatalf("lost coupon calendar guard: %v", err)
		}
		bondCheck(t, flow.Close())
	}
	bondCheck(t, config.Settings.SetEvaluationDate(testDate(t, 10, 10, 2027)))
	starts := []Date{testDate(t, 5, 10, 2026), testDate(t, 5, 1, 2027), testDate(t, 5, 4, 2027), testDate(t, 5, 7, 2027), testDate(t, 5, 10, 2027)}
	for i, position := range []int{0, 2, 3, 5} {
		flow, err := leg.At(position)
		bondCheck(t, err)
		if _, err := flow.Amount(); err == nil {
			t.Fatal("missing past fixing accepted")
		}
		fixing, err := DateFromSerial(starts[i].Serial() - 2)
		bondCheck(t, err)
		bondCheck(t, index.AddFixing(fixing, 0.037))
		amount, err := flow.Amount()
		bondCheck(t, err)
		amortNear(t, amount, config.Notionals[i]*0.037*float64(starts[i+1].Serial()-starts[i].Serial())/360)
		bondCheck(t, flow.Close())
	}
	bondCheck(t, s.Close())
	if _, err := leg.Len(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
}

func TestAmortizingFloatingDomainsSessionsAndClosedObjects(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	config, _, _ := amortMarket(t, s, false)
	for _, balances := range [][]float64{nil, {1000}, {1000, 0, 800, 500}, {1000, -1, 800, 500}, {1000, 1100, 800, 500}, {1000, math.NaN(), 800, 500}, {math.Inf(1), 800, 800, 500}} {
		bad := config
		bad.Notionals = balances
		if _, err := s.NewAmortizingFloatingRateBond(bad); err == nil {
			t.Fatal("invalid balances accepted")
		}
	}
	other, err := NewSession()
	bondCheck(t, err)
	defer other.Close()
	foreign, _, _ := amortMarket(t, other, false)
	bad := config
	bad.Index = foreign.Index
	if _, err := s.NewAmortizingFloatingRateBond(bad); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("mixed session: %v", err)
	}
	bondCheck(t, config.Index.Close())
	if _, err := s.NewAmortizingFloatingRateBond(config); err == nil {
		t.Fatal("closed index accepted")
	}
}

func TestAmortizingFloatingDateSpanBounds(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	config, _, _ := amortMarket(t, s, false)
	bound := uint32(testDate(t, 31, 12, 2199).Serial() - testDate(t, 1, 1, 1901).Serial())
	for _, change := range []struct{ settlement, fixing uint32 }{{bound + 1, 0}, {^uint32(0), 0}, {0, bound + 1}, {0, ^uint32(0)}} {
		bad := config
		bad.SettlementDays = change.settlement
		bad.FixingDays = &change.fixing
		_, err := s.NewAmortizingFloatingRateBond(bad)
		var native *Error
		if !errors.As(err, &native) || native.Code != 1 {
			t.Fatalf("typed date-bound error: %v", err)
		}
	}
	calendar, err := s.NullCalendar()
	bondCheck(t, err)
	currency, err := s.USD()
	bondCheck(t, err)
	index, err := s.NewIborIndex(IborIndexConfig{FamilyName: "OversizedAmortizingIndex", Tenor: Period{6, Months}, SettlementDays: bound + 1, Currency: currency, FixingCalendar: calendar, Convention: Following, DayCounter: config.AccrualDayCounter, Settings: config.Settings})
	bondCheck(t, err)
	zero := uint32(0)
	config.Index, config.FixingDays = index, &zero
	_, err = s.NewAmortizingFloatingRateBond(config)
	var native *Error
	if !errors.As(err, &native) || native.Code != 1 {
		t.Fatalf("typed inherited index-lag error: %v", err)
	}
}
