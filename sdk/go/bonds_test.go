package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"sync"
	"testing"
)

type bondSnapshot struct {
	NPV             float64     `json:"npv"`
	SettlementValue float64     `json:"settlement_value"`
	Clean           float64     `json:"clean"`
	Dirty           float64     `json:"dirty"`
	Accrued         float64     `json:"accrued"`
	Settlement      int32       `json:"settlement"`
	Notional        float64     `json:"notional"`
	Expired         bool        `json:"expired"`
	Cashflows       [][]float64 `json:"cashflows"`
}

func bondCheck(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}
func bondNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > 2e-12*math.Max(1, math.Abs(want)) {
		t.Fatalf("got %.17g want %.17g", got, want)
	}
}
func bondFixture(t *testing.T, s *Session, kind string, face float64) (*Bond, *SimpleQuote, *Settings, *Calendar, *Schedule, *DayCounter, *YieldTermStructure, *DiscountingBondEngine) {
	t.Helper()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	today := testDate(t, 15, 1, 2024)
	bondCheck(t, settings.SetEvaluationDate(today))
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	quote, e := s.NewSimpleQuote(.04)
	bondCheck(t, e)
	curve, e := s.NewFlatForwardFromQuote(today, quote, dc)
	bondCheck(t, e)
	backward := Backward
	schedule, e := s.NewSchedule(ScheduleConfig{Start: today, End: testDate(t, 15, 1, 2027), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &backward})
	bondCheck(t, e)
	engine, e := s.NewDiscountingBondEngine(curve, settings, nil)
	bondCheck(t, e)
	config := BondConfig{SettlementDays: 2, FaceAmount: face, Settings: settings, IssueDate: &today}
	var bond *Bond
	if kind == "zero" {
		config.Calendar = cal
		zero, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: config, MaturityDate: testDate(t, 15, 1, 2027)})
		bondCheck(t, e)
		bond = &zero.Bond
	} else {
		cfg := FixedRateBondConfig{BondConfig: config, Schedule: schedule, Coupons: []float64{.05}, AccrualDayCounter: dc}
		if kind == "ex_coupon" {
			cfg.ExCouponPeriod = &Period{7, Days}
			cfg.ExCouponCalendar = cal
			bondCheck(t, settings.SetEvaluationDate(testDate(t, 9, 7, 2024)))
		}
		fixed, e := s.NewFixedRateBond(cfg)
		bondCheck(t, e)
		bond = &fixed.Bond
		frequency, e := fixed.Frequency()
		bondCheck(t, e)
		if frequency != Semiannual {
			t.Fatalf("frequency %d", frequency)
		}
		first, e := fixed.FirstPeriodDayCounter()
		bondCheck(t, e)
		if first != nil {
			t.Fatal("unexpected first-period counter")
		}
		inspected, e := fixed.DayCounter()
		bondCheck(t, e)
		bondCheck(t, inspected.Close())
	}
	bondCheck(t, bond.SetEngine(engine))
	return bond, quote, settings, cal, schedule, dc, curve, engine
}
func bondAssertSnapshot(t *testing.T, b *Bond, w bondSnapshot) {
	t.Helper()
	checks := []struct {
		read func() (float64, error)
		want float64
	}{{b.NPV, w.NPV}, {b.SettlementValue, w.SettlementValue}, {b.CleanPrice, w.Clean}, {b.DirtyPrice, w.Dirty}, {func() (float64, error) { return b.AccruedAmount(nil) }, w.Accrued}, {func() (float64, error) { return b.Notional(nil) }, w.Notional}}
	for _, check := range checks {
		got, e := check.read()
		bondCheck(t, e)
		bondNear(t, got, check.want)
	}
	settlement, e := b.SettlementDate(nil)
	bondCheck(t, e)
	if settlement.Serial() != w.Settlement {
		t.Fatalf("settlement %d want %d", settlement.Serial(), w.Settlement)
	}
	expired, e := b.IsExpired()
	bondCheck(t, e)
	if expired != w.Expired {
		t.Fatal("expiry mismatch")
	}
	leg, e := b.Cashflows()
	bondCheck(t, e)
	defer leg.Close()
	count, e := leg.Len()
	bondCheck(t, e)
	if count != len(w.Cashflows) {
		t.Fatal("cashflow count")
	}
	for i, row := range w.Cashflows {
		flow, e := leg.At(i)
		bondCheck(t, e)
		d, e := flow.Date()
		bondCheck(t, e)
		if d.Serial() != int32(row[0]) {
			t.Fatal("cashflow date")
		}
		amount, e := flow.Amount()
		bondCheck(t, e)
		bondNear(t, amount, row[1])
		bondCheck(t, flow.Close())
	}
}
func TestBondNativeOracleAndRetainedLifecycle(t *testing.T) {
	data, e := os.ReadFile("testdata/bonds_oracle.json")
	bondCheck(t, e)
	var oracle struct {
		Version string                             `json:"quantlib_version"`
		Cases   map[string]map[string]bondSnapshot `json:"cases"`
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	if oracle.Version != "1.43" {
		t.Fatal("oracle version")
	}
	for kind, cases := range oracle.Cases {
		t.Run(kind, func(t *testing.T) {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			b, q, settings, cal, schedule, dc, curve, engine := bondFixture(t, s, kind, 1000)
			bondAssertSnapshot(t, b, cases["initial"])
			maturity, e := b.MaturityDate()
			bondCheck(t, e)
			if maturity != testDate(t, 15, 1, 2027) {
				t.Fatal("maturity")
			}
			issue, e := b.IssueDate()
			bondCheck(t, e)
			if issue == nil || *issue != testDate(t, 15, 1, 2024) {
				t.Fatal("issue")
			}
			for _, close := range []func() error{cal.Close, schedule.Close, dc.Close, curve.Close, engine.Close} {
				bondCheck(t, close())
			}
			bondCheck(t, b.Freeze())
			bondCheck(t, q.SetValue(.055))
			bondAssertSnapshot(t, b, cases["initial"])
			bondCheck(t, b.Unfreeze())
			bondAssertSnapshot(t, b, cases["updated"])
			bondCheck(t, b.Recalculate())
			bondAssertSnapshot(t, b, cases["updated"])
			retained, e := b.Redemptions()
			bondCheck(t, e)
			bondCheck(t, b.Close())
			flow, e := retained.At(0)
			bondCheck(t, e)
			amount, e := flow.Amount()
			bondCheck(t, e)
			bondNear(t, amount, 1000)
			bondCheck(t, flow.Close())
			bondCheck(t, retained.Close())
			if _, e := b.NPV(); e == nil {
				t.Fatal("released bond priced")
			}
			bondCheck(t, b.Close())
			bondCheck(t, settings.Close())
			bondCheck(t, q.Close())
		})
	}
}
func TestBondExpiryScalingErrorsAndConcurrentCalls(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	b, _, settings, cal, _, dc, curve, _ := bondFixture(t, s, "zero", 1e306)
	price, e := b.DirtyPrice()
	bondCheck(t, e)
	bondNear(t, price, 88.70176388033903)
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Go(func() {
			for j := 0; j < 10; j++ {
				v, e := b.NPV()
				if e != nil || (math.IsNaN(v) || math.IsInf(v, 0)) {
					t.Errorf("concurrent price %g %v", v, e)
				}
			}
		})
	}
	wg.Wait()
	other, e := s.NewSettings()
	bondCheck(t, e)
	bondCheck(t, other.SetEvaluationDate(testDate(t, 15, 1, 2024)))
	wrong, e := s.NewDiscountingBondEngine(curve, other, nil)
	bondCheck(t, e)
	if e := b.SetEngine(wrong); e == nil {
		t.Fatal("mismatched settings accepted")
	}
	s2, e := NewSession()
	bondCheck(t, e)
	defer s2.Close()
	foreign, e := s2.NewSettings()
	bondCheck(t, e)
	if _, e := s.NewDiscountingBondEngine(curve, foreign, nil); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("cross session %v", e)
	}
	for _, face := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		if _, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{Calendar: cal, Settings: settings, FaceAmount: face}, MaturityDate: testDate(t, 15, 1, 2027)}); e == nil {
			t.Fatal("invalid face accepted")
		}
	}
	if _, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{Calendar: cal, Settings: settings, FaceAmount: 100, SettlementDays: math.MaxUint32}, MaturityDate: testDate(t, 15, 1, 2027)}); e == nil {
		t.Fatal("huge days accepted")
	}
	if _, e := b.AccruedAmount(&Date{}); e == nil {
		t.Fatal("invalid date accepted")
	}
	bondCheck(t, settings.SetEvaluationDate(testDate(t, 31, 12, 2199)))
	if _, e := b.SettlementDate(nil); e == nil {
		t.Fatal("max date overflow accepted")
	}
	if _, e := b.NPV(); e == nil {
		t.Fatal("max date priced")
	}
	bondCheck(t, settings.SetEvaluationDate(testDate(t, 15, 1, 2024)))
	if _, e := b.NPV(); e != nil {
		t.Fatalf("session poisoned %v", e)
	}
	bondCheck(t, settings.SetEvaluationDate(testDate(t, 16, 1, 2027)))
	expired, e := b.IsExpired()
	bondCheck(t, e)
	if !expired {
		t.Fatal("not expired")
	}
	price, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, price, 0)
	missing, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{Calendar: cal, Settings: other, FaceAmount: 100}, MaturityDate: testDate(t, 15, 1, 2027)})
	bondCheck(t, e)
	if _, e := missing.NPV(); e == nil {
		t.Fatal("missing engine accepted")
	}
	_ = dc
}
func TestBondReferenceEventPolicyHonorsFreeze(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	today := testDate(t, 15, 1, 2024)
	bondCheck(t, settings.SetEvaluationDate(today))
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	curve, e := s.NewFlatForward(today, .04, dc)
	bondCheck(t, e)
	engine, e := s.NewDiscountingBondEngine(curve, settings, nil)
	bondCheck(t, e)
	b, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{Calendar: cal, Settings: settings, FaceAmount: 100}, MaturityDate: today})
	bondCheck(t, e)
	bondCheck(t, b.SetEngine(engine))
	v, e := b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 0)
	bondCheck(t, b.Freeze())
	bondCheck(t, settings.SetIncludeReferenceDateEvents(true))
	include, e := settings.IncludeReferenceDateEvents()
	bondCheck(t, e)
	if !include {
		t.Fatal("flag not set")
	}
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 0)
	bondCheck(t, b.Unfreeze())
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 100)
	no := false
	bondCheck(t, settings.SetIncludeTodaysCashFlows(&no))
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 0)
	bondCheck(t, settings.SetIncludeTodaysCashFlows(nil))
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 100)

	bondCheck(t, b.Freeze())
	bondCheck(t, settings.SetIncludeReferenceDateEvents(false))
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 0)
	bondCheck(t, b.Unfreeze())
	yes := true
	bondCheck(t, settings.SetIncludeTodaysCashFlows(&yes))
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 100)
	engineFalse, e := s.NewDiscountingBondEngine(curve, settings, &no)
	bondCheck(t, e)
	bondCheck(t, b.SetEngine(engineFalse))
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 100)
}
