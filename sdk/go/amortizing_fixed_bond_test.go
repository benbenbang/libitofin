package itofin

import (
	"bufio"
	"encoding/json"
	"errors"
	"math"
	"os"
	"strings"
	"sync"
	"testing"
)

type amortizingFixedSnapshot struct {
	Forward        []float64   `json:"forward"`
	Values         []float64   `json:"values"`
	Settlement     int32       `json:"settlement"`
	Maturity       int32       `json:"maturity"`
	Expired        bool        `json:"expired"`
	Flows          [][]float64 `json:"flows"`
	Redemptions    [][]float64 `json:"redemptions"`
	Yield          *float64    `json:"yield"`
	DatedNotionals []float64   `json:"dated_notionals"`
}
type amortizingFixedRow struct {
	Name           string                  `json:"name"`
	Notionals      []float64               `json:"notionals"`
	When           []int                   `json:"when"`
	ExDays         int32                   `json:"ex_days"`
	SettlementDays uint32                  `json:"settlement_days"`
	Initial        amortizingFixedSnapshot `json:"initial"`
	Updated        amortizingFixedSnapshot `json:"updated"`
}

func amortizingFixedConfig(t *testing.T, s *Session) AmortizingFixedRateBondConfig {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(testDate(t, 18, 1, 2024)))
	cal := pricingMust(s.WeekendsOnly())
	dc := pricingMust(s.Actual360())
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: testDate(t, 17, 1, 2024), End: testDate(t, 17, 1, 2027), Frequency: Annual, Calendar: cal, Convention: Unadjusted}))
	return AmortizingFixedRateBondConfig{SettlementDays: 2, Notionals: []float64{1000, 750, 400}, Settings: settings, Calendar: cal, Schedule: schedule, Coupons: []float64{.045, .052, .039}, AccrualDayCounter: dc, IssueDate: pricingPtr(testDate(t, 17, 1, 2024))}
}
func amortizingFixedAssert(t *testing.T, b *FixedRateBond, w amortizingFixedSnapshot, dc *DayCounter, curve *YieldTermStructure) {
	t.Helper()
	reads := []func() (float64, error){b.NPV, b.SettlementValue, b.CleanPrice, b.DirtyPrice, func() (float64, error) { return b.AccruedAmount(nil) }, func() (float64, error) { return b.Notional(nil) }}
	for i, read := range reads {
		got, err := read()
		bondCheck(t, err)
		bondNear(t, got, w.Values[i])
	}
	settlement := pricingMust(b.SettlementDate(nil))
	if settlement.Serial() != w.Settlement {
		t.Fatal("settlement")
	}
	maturity := pricingMust(b.MaturityDate())
	if maturity.Serial() != w.Maturity {
		t.Fatal("contractual maturity")
	}
	expired := pricingMust(b.IsExpired())
	if expired != w.Expired {
		t.Fatal("expiry")
	}
	for i, get := range []func() (*Leg, error){b.Cashflows, b.Redemptions} {
		leg := pricingMust(get())
		expected := w.Flows
		if i == 1 {
			expected = w.Redemptions
		}
		if pricingMust(leg.Len()) != len(expected) {
			t.Fatal("leg length")
		}
		for j, row := range expected {
			flow := pricingMust(leg.At(j))
			if pricingMust(flow.Date()).Serial() != int32(row[0]) {
				t.Fatal("payment date")
			}
			bondNear(t, pricingMust(flow.Amount()), row[1])
			bondCheck(t, flow.Close())
		}
		bondCheck(t, leg.Close())
	}
	dates := [][3]int{{16, 1, 2025}, {17, 1, 2025}, {18, 1, 2025}, {18, 1, 2026}, {19, 1, 2026}, {20, 1, 2026}, {17, 1, 2027}, {18, 1, 2027}, {19, 1, 2027}}
	for i, date := range dates {
		d := testDate(t, date[0], date[1], date[2])
		bondNear(t, pricingMust(b.Notional(&d)), w.DatedNotionals[i])
	}
	if w.Forward != nil {
		cal := pricingMust(b.Calendar())
		f := pricingMust(b.session.NewBondForward(BondForwardConfig{Bond: &b.Bond, ValueDate: testDate(t, 18, 1, 2024), DeliveryDate: testDate(t, 19, 1, 2026), Position: PositionLong, Strike: 105, DayCounter: dc, Calendar: cal, Convention: Unadjusted, Discount: curve}))
		for i, read := range []func() (float64, error){f.NPV, f.FairForwardPrice, f.CleanForwardPrice} {
			bondNear(t, pricingMust(read()), w.Forward[i])
		}
		bondCheck(t, f.Close())
		bondCheck(t, cal.Close())
	}
	if w.Yield != nil {
		config := BondYieldSolveConfig{BondYieldConfig: BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Annual}, AnalyticsSolverConfig: AnalyticsSolverConfig{Accuracy: pricingPtr(1e-12), MaxEvaluations: pricingPtr(uint(1000)), Guess: pricingPtr(.04)}}
		got := pricingMust(b.YieldRate(w.Values[2], config))
		if !isAmortizingFixedNear(got, *w.Yield, 3e-10) {
			t.Fatalf("yield %.17g want %.17g", got, *w.Yield)
		}
		bondNear(t, pricingMust(b.CleanPriceFromYield(got, config.BondYieldConfig)), w.Values[2])
	}
}
func isAmortizingFixedNear(got, want, tolerance float64) bool {
	return !math.IsNaN(got) && !math.IsInf(got, 0) && !math.IsNaN(want) && !math.IsInf(want, 0) && math.Abs(got-want) <= tolerance
}
func TestAmortizingFixedNativeOracle(t *testing.T) {
	file, err := os.Open("testdata/amortizing_fixed_native.jsonl")
	bondCheck(t, err)
	defer file.Close()
	scanner := bufio.NewScanner(file)
	if !scanner.Scan() {
		t.Fatal("missing provenance")
	}
	var provenance struct {
		Version string `json:"compiled_quantlib_version"`
		Same    bool   `json:"source_is_compiled_wheel"`
	}
	bondCheck(t, json.Unmarshal(scanner.Bytes(), &provenance))
	if provenance.Version != "1.43" || provenance.Same {
		t.Fatal("oracle identity")
	}
	count := 0
	for scanner.Scan() {
		var row amortizingFixedRow
		bondCheck(t, json.Unmarshal(scanner.Bytes(), &row))
		count++
		t.Run(row.Name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			a := amortizingFixedConfig(t, s)
			a.Notionals = row.Notionals
			a.SettlementDays = row.SettlementDays
			bondCheck(t, a.Settings.SetEvaluationDate(testDate(t, row.When[0], row.When[1], row.When[2])))
			if row.ExDays != 0 {
				a.ExCouponPeriod = &Period{row.ExDays, Days}
				a.ExCouponCalendar = a.Calendar
			}
			b := pricingMust(s.NewAmortizingFixedRateBond(a))
			quote := pricingMust(s.NewSimpleQuote(.031))
			curve := pricingMust(s.NewFlatForwardFromQuote(testDate(t, 18, 1, 2024), quote, a.AccrualDayCounter))
			engine := pricingMust(s.NewDiscountingBondEngine(curve, a.Settings, nil))
			bondCheck(t, b.SetEngine(engine))
			amortizingFixedAssert(t, b, row.Initial, a.AccrualDayCounter, curve)
			bondCheck(t, quote.SetValue(.067))
			amortizingFixedAssert(t, b, row.Updated, a.AccrualDayCounter, curve)
		})
	}
	bondCheck(t, scanner.Err())
	if count != 108 {
		t.Fatalf("native rows %d", count)
	}
	for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if isAmortizingFixedNear(bad, 0, 1) || isAmortizingFixedNear(0, bad, 1) {
			t.Fatal("nonfinite accepted")
		}
	}
}
func TestAmortizingFixedDomainsRetentionAndConsumers(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	a := amortizingFixedConfig(t, s)
	invalid := [][]float64{nil, {1000}, {1000, 750, 400, 100}, {1000, 1001, 400}, {1000, 0, 0}, {1000, -1, 0}, {math.Inf(1), 750, 400}, {1000, math.NaN(), 400}}
	for _, balances := range invalid {
		bad := a
		bad.Notionals = balances
		if _, err := s.NewAmortizingFixedRateBond(bad); err == nil {
			t.Fatal("invalid balances accepted")
		}
	}
	foreign := pricingMust(NewSession())
	defer foreign.Close()
	bad := a
	bad.Settings = pricingMust(foreign.NewSettings())
	if _, err := s.NewAmortizingFixedRateBond(bad); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign session %v", err)
	}
	b := pricingMust(s.NewAmortizingFixedRateBond(a))
	quote := pricingMust(s.NewSimpleQuote(.031))
	curve := pricingMust(s.NewFlatForwardFromQuote(testDate(t, 18, 1, 2024), quote, a.AccrualDayCounter))
	engine := pricingMust(s.NewDiscountingBondEngine(curve, a.Settings, nil))
	bondCheck(t, b.SetEngine(engine))
	index := pricingMust(s.NewIborIndex(IborIndexConfig{FamilyName: "AmortizingBoundary", Tenor: Period{1, Years}, Currency: pricingMust(s.EUR()), FixingCalendar: a.Calendar, Convention: Unadjusted, DayCounter: a.AccrualDayCounter, Forwarding: curve, Settings: a.Settings}))
	if _, err := s.NewAssetSwap(AssetSwapConfig{Bond: &b.Bond, Index: index, CleanPrice: 100, PayBondCoupon: true}); err == nil || !strings.Contains(err.Error(), "bullet") {
		t.Fatalf("amortizing asset swap boundary: %v", err)
	}
	retained := pricingMust(b.Redemptions())
	initial := pricingMust(b.NPV())
	if !isAmortizingFixedNear(initial, initial, 0) || initial <= 0 {
		t.Fatal("initial NPV")
	}
	bondCheck(t, b.Freeze())
	bondCheck(t, quote.SetValue(.067))
	bondNear(t, pricingMust(b.NPV()), initial)
	bondCheck(t, b.Unfreeze())
	if got := pricingMust(b.NPV()); !isAmortizingFixedNear(got, got, 0) || got >= initial {
		t.Fatal("live quote")
	}
	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 10; j++ {
				got, err := b.NPV()
				if err != nil || !isAmortizingFixedNear(got, got, 0) {
					t.Errorf("concurrent read %g %v", got, err)
				}
			}
		}()
	}
	wg.Wait()
	for _, close := range []func() error{a.Calendar.Close, a.Schedule.Close, a.AccrualDayCounter.Close, curve.Close, engine.Close, a.Settings.Close, quote.Close} {
		bondCheck(t, close())
	}
	got := pricingMust(b.NPV())
	if !isAmortizingFixedNear(got, got, 0) {
		t.Fatal("retention")
	}
	bondCheck(t, b.Close())
	flow := pricingMust(retained.At(0))
	bondNear(t, pricingMust(flow.Amount()), 250)
	bondCheck(t, flow.Close())
	bondCheck(t, retained.Close())
	_, err := b.NPV()
	var native *Error
	if !errors.As(err, &native) || native.Code != 2 {
		t.Fatalf("released bond handle %v", err)
	}
	bondCheck(t, s.Close())
	if _, err := b.NPV(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session %v", err)
	}
}

func TestAmortizingFixedSilentTodayFlagsAndExCouponPrincipal(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	a := amortizingFixedConfig(t, s)
	a.SettlementDays = 0
	today := testDate(t, 17, 1, 2025)
	bondCheck(t, a.Settings.SetEvaluationDate(today))
	b := pricingMust(s.NewAmortizingFixedRateBond(a))
	curve := pricingMust(s.NewFlatForward(today, .031, a.AccrualDayCounter))
	engine := pricingMust(s.NewDiscountingBondEngine(curve, a.Settings, nil))
	bondCheck(t, b.SetEngine(engine))
	before := pricingMust(b.NPV())
	bondCheck(t, b.Freeze())
	bondCheck(t, a.Settings.SetIncludeReferenceDateEvents(true))
	bondCheck(t, a.Settings.SetIncludeTodaysCashFlows(pricingPtr(true)))
	bondNear(t, pricingMust(b.NPV()), before)
	bondCheck(t, b.Unfreeze())
	bondNear(t, pricingMust(b.NPV())-before, 250+1000*.045*366/360)
	bondNear(t, pricingMust(b.Notional(nil)), 750)
	bondCheck(t, a.Settings.SetIncludeTodaysCashFlows(pricingPtr(false)))
	bondNear(t, pricingMust(b.NPV()), before)
	date := testDate(t, 10, 1, 2025)
	bondCheck(t, a.Settings.SetEvaluationDate(date))
	bondCheck(t, a.Settings.SetIncludeReferenceDateEvents(false))
	bondCheck(t, a.Settings.SetIncludeTodaysCashFlows(nil))
	plain := pricingMust(s.NewAmortizingFixedRateBond(a))
	a.ExCouponPeriod = &Period{7, Days}
	a.ExCouponCalendar = a.Calendar
	ex := pricingMust(s.NewAmortizingFixedRateBond(a))
	curve = pricingMust(s.NewFlatForward(date, .031, a.AccrualDayCounter))
	engine = pricingMust(s.NewDiscountingBondEngine(curve, a.Settings, nil))
	bondCheck(t, plain.SetEngine(engine))
	bondCheck(t, ex.SetEngine(engine))
	bondNear(t, pricingMust(plain.NPV())-pricingMust(ex.NPV()), 1000*.045*366/360*math.Exp(-.031*7/360))
	leg := pricingMust(ex.Redemptions())
	defer leg.Close()
	flow := pricingMust(leg.At(0))
	defer flow.Close()
	bondNear(t, pricingMust(flow.Amount()), 250)
}

func TestAmortizingFixedExtremeBalances(t *testing.T) {
	for _, scale := range []float64{1e-250, 1e300} {
		s := pricingMust(NewSession())
		a := amortizingFixedConfig(t, s)
		a.Notionals = []float64{scale, scale * .75, scale * .4}
		a.Coupons = []float64{0}
		b := pricingMust(s.NewAmortizingFixedRateBond(a))
		curve := pricingMust(s.NewFlatForward(testDate(t, 18, 1, 2024), 0, a.AccrualDayCounter))
		engine := pricingMust(s.NewDiscountingBondEngine(curve, a.Settings, nil))
		bondCheck(t, b.SetEngine(engine))
		ratio := pricingMust(b.NPV()) / scale
		if !isAmortizingFixedNear(ratio, 1, 2e-13) {
			t.Fatal("currency scale")
		}
		bondNear(t, pricingMust(b.DirtyPrice()), 100)
		bondCheck(t, a.Settings.SetEvaluationDate(testDate(t, 20, 1, 2026)))
		ratio = pricingMust(b.Notional(nil)) / (scale * .4)
		if !isAmortizingFixedNear(ratio, 1, 2e-13) {
			t.Fatal("current notional")
		}
		bondNear(t, pricingMust(b.DirtyPrice()), 100)
		bondCheck(t, s.Close())
	}
}

func TestAmortizingFixedPreservesExistingBulletNontradableYield(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	a := amortizingFixedConfig(t, s)
	bondCheck(t, a.Settings.SetEvaluationDate(testDate(t, 19, 1, 2027)))
	fixed := pricingMust(s.NewFixedRateBond(FixedRateBondConfig{BondConfig: BondConfig{FaceAmount: 1000, Settings: a.Settings}, Schedule: a.Schedule, Coupons: []float64{0.04}, AccrualDayCounter: a.AccrualDayCounter}))
	zero := pricingMust(s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{FaceAmount: 1000, Settings: a.Settings, Calendar: a.Calendar}, MaturityDate: testDate(t, 17, 1, 2027)}))
	for _, b := range []*Bond{&fixed.Bond, &zero.Bond} {
		bondNear(t, pricingMust(b.Notional(nil)), 0)
		for _, kind := range []BondPriceType{Clean, Dirty} {
			_, err := b.YieldRate(0, BondYieldSolveConfig{BondYieldConfig: BondYieldConfig{DayCounter: a.AccrualDayCounter, Compounding: Compounded, Frequency: Annual}, PriceType: kind})
			var typed *Error
			if !errors.As(err, &typed) || !strings.Contains(typed.Message, "non tradable") {
				t.Fatalf("existing analytics contract: %v", err)
			}
		}
	}
}
