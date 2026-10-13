package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"reflect"
	"sync"
	"testing"
)

type callableFoundationSnapshot struct {
	Settlement  int32       `json:"settlement"`
	Maturity    int32       `json:"maturity"`
	Issue       int32       `json:"issue"`
	Notional    float64     `json:"notional"`
	Accrued     float64     `json:"accrued"`
	Cashflows   [][]float64 `json:"cashflows"`
	Callability [][]float64 `json:"callability"`
	CleanCurve  float64     `json:"clean_curve"`
	DirtyCurve  float64     `json:"dirty_curve"`
	CleanYield  float64     `json:"clean_yield"`
	DirtyYield  float64     `json:"dirty_yield"`
	CleanSpread float64     `json:"clean_spread"`
	DirtySpread float64     `json:"dirty_spread"`
	Yield       float64     `json:"yield"`
	Spread      float64     `json:"spread"`
}

func callableFoundationFixture(t *testing.T, s *Session, kind string) (*Bond, func() (CallabilitySchedule, error), *Settings, *DayCounter, *YieldTermStructure) {
	t.Helper()
	today := testDate(t, 15, 1, 2024)
	maturity := testDate(t, 15, 1, 2027)
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(today))
	cal := pricingMust(s.NullCalendar())
	dc := pricingMust(s.Actual365Fixed())
	curve := pricingMust(s.NewFlatForward(today, .04, dc))
	backward := Backward
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: today, End: maturity, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &backward}))
	calls := CallabilitySchedule{
		pricingMust(NewCallability(100, Clean, CallabilityCall, testDate(t, 15, 1, 2025))),
		pricingMust(NewCallability(95, Dirty, CallabilityPut, testDate(t, 15, 1, 2025))),
		pricingMust(NewCallability(0, Dirty, CallabilityCall, today)),
	}
	config := BondConfig{SettlementDays: 2, FaceAmount: 1000, Settings: settings, IssueDate: &today}
	var bond *Bond
	var read func() (CallabilitySchedule, error)
	var retained *DayCounter
	if kind == "zero" {
		config.Calendar = cal
		zero := pricingMust(s.NewCallableZeroCouponBond(CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: config, MaturityDate: maturity}, DayCounter: dc, Callability: calls}))
		bond, read, retained = &zero.Bond, zero.Callability, pricingMust(zero.DayCounter())
	} else {
		cfg := CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: config, Schedule: schedule, Coupons: []float64{.05}, AccrualDayCounter: dc}, Callability: calls}
		if kind == "ex_coupon" {
			cfg.ExCouponPeriod, cfg.ExCouponCalendar = &Period{7, Days}, cal
			bondCheck(t, settings.SetEvaluationDate(testDate(t, 9, 7, 2024)))
		}
		fixed := pricingMust(s.NewCallableFixedRateBond(cfg))
		if pricingMust(fixed.Frequency()) != Semiannual {
			t.Fatal("fixed frequency")
		}
		bond, read, retained = &fixed.Bond, fixed.Callability, pricingMust(fixed.DayCounter())
	}
	calls[0].Price = 999
	bondCheck(t, schedule.Close())
	bondCheck(t, cal.Close())
	bondCheck(t, dc.Close())
	return bond, read, settings, retained, curve
}
func TestCallableBondFoundationQuantLibOracleAndLifetime(t *testing.T) {
	data := pricingMust(os.ReadFile("testdata/callable_bonds_oracle.json"))
	var oracle struct {
		Version string                                `json:"quantlib_version"`
		Cases   map[string]callableFoundationSnapshot `json:"cases"`
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	if oracle.Version != "1.43" {
		t.Fatal("native oracle version")
	}
	for kind, want := range oracle.Cases {
		t.Run(kind, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			b, read, settings, dc, curve := callableFoundationFixture(t, s, kind)
			settlement := pricingMust(b.SettlementDate(nil))
			if settlement.Serial() != want.Settlement || pricingMust(b.MaturityDate()).Serial() != want.Maturity || pricingMust(b.IssueDate()).Serial() != want.Issue {
				t.Fatal("bond dates")
			}
			if pricingMust(b.SettlementDays()) != 2 || pricingMust(b.IsExpired()) || pricingMust(b.IsCalculated()) {
				t.Fatal("initial state")
			}
			bondNear(t, pricingMust(b.Notional(nil)), want.Notional)
			bondNear(t, pricingMust(b.AccruedAmount(nil)), want.Accrued)
			calls := pricingMust(read())
			for i, row := range want.Callability {
				bondNear(t, calls[i].Price, row[0])
				if calls[i].PriceType != BondPriceType(row[1]) || calls[i].Type != CallabilityType(row[2]) || calls[i].Date.Serial() != int32(row[3]) {
					t.Fatal("callability order/value")
				}
			}
			calls[0].Price = 777
			if pricingMust(read())[0].Price != 100 {
				t.Fatal("schedule was not copied")
			}
			leg := pricingMust(b.Cashflows())
			if pricingMust(leg.Len()) != len(want.Cashflows) {
				t.Fatal("cashflow count")
			}
			for i, row := range want.Cashflows {
				flow := pricingMust(leg.At(i))
				if pricingMust(flow.Date()).Serial() != int32(row[0]) {
					t.Fatal("cashflow date")
				}
				bondNear(t, pricingMust(flow.Amount()), row[1])
				bondCheck(t, flow.Close())
			}
			calendar := pricingMust(b.Calendar())
			bondCheck(t, calendar.Close())
			redemptions := pricingMust(b.Redemptions())
			if pricingMust(redemptions.Len()) != 1 {
				t.Fatal("redemption count")
			}
			bondCheck(t, redemptions.Close())
			y := BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Semiannual}
			z := BondZSpreadConfig{Discount: curve, Compounding: Compounded, Frequency: Semiannual}
			checks := []struct{ value, want float64 }{
				{pricingMust(b.CleanPriceFromCurve(curve, nil)), want.CleanCurve},
				{pricingMust(b.DirtyPriceFromCurve(curve, nil)), want.DirtyCurve},
				{pricingMust(b.CleanPriceFromYield(.047, y)), want.CleanYield},
				{pricingMust(b.DirtyPriceFromYield(.047, y)), want.DirtyYield},
				{pricingMust(b.CleanPriceAtZSpread(.006, z)), want.CleanSpread},
				{pricingMust(b.DirtyPriceAtZSpread(.006, z)), want.DirtySpread},
			}
			for _, check := range checks {
				bondNear(t, check.value, check.want)
			}
			accuracy, count := 1e-12, uint(1000)
			solver := AnalyticsSolverConfig{Accuracy: &accuracy, MaxEvaluations: &count}
			bondNear(t, pricingMust(b.YieldRate(want.CleanYield, BondYieldSolveConfig{BondYieldConfig: y, PriceType: Clean, AnalyticsSolverConfig: solver})), want.Yield)
			bondNear(t, pricingMust(b.ZSpread(want.CleanSpread, BondZSpreadSolveConfig{BondZSpreadConfig: z, PriceType: Clean, AnalyticsSolverConfig: solver})), want.Spread)
			if pricingMust(b.IsCalculated()) {
				t.Fatal("pure analytics changed lazy state")
			}
			for _, price := range []func() (float64, error){b.NPV, b.SettlementValue, b.CleanPrice, b.DirtyPrice} {
				if _, err := price(); err == nil {
					t.Fatal("missing engine accepted")
				}
			}
			engine := pricingMust(s.NewDiscountingBondEngine(curve, settings, nil))
			if err := b.SetEngine(engine); err == nil {
				t.Fatal("generic discount engine accepted")
			}
			bondCheck(t, b.Freeze())
			if _, err := b.NPV(); err == nil {
				t.Fatal("frozen missing result accepted")
			}
			if err := b.Recalculate(); err == nil {
				t.Fatal("missing-engine recalculation accepted")
			}
			bondCheck(t, b.Unfreeze())
			bondCheck(t, settings.SetEvaluationDate(testDate(t, 16, 1, 2027)))
			for _, price := range []func() (float64, error){b.NPV, b.SettlementValue, b.CleanPrice, b.DirtyPrice} {
				bondNear(t, pricingMust(price()), 0)
			}
			if !pricingMust(b.IsExpired()) || !pricingMust(b.IsCalculated()) {
				t.Fatal("expired state")
			}
			bondCheck(t, b.Close())
			if _, err := read(); err == nil {
				t.Fatal("closed callable inspected")
			}
			flow := pricingMust(leg.At(0))
			bondNear(t, pricingMust(flow.Amount()), want.Cashflows[0][1])
			bondCheck(t, flow.Close())
			bondCheck(t, leg.Close())
		})
	}
}
func TestCallableBondFoundationValidationAndSessions(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	s2 := pricingMust(NewSession())
	defer s2.Close()
	today := testDate(t, 15, 1, 2024)
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(today))
	cal := pricingMust(s.NullCalendar())
	dc := pricingMust(s.Actual365Fixed())
	foreignDC := pricingMust(s2.Actual365Fixed())
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: today, End: testDate(t, 15, 1, 2027), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted}))
	fixed := CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: BondConfig{FaceAmount: 1000, Settings: settings}, Schedule: schedule, Coupons: []float64{.05}, AccrualDayCounter: dc}}
	zero := CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: BondConfig{FaceAmount: 1000, Settings: settings, Calendar: cal}, MaturityDate: testDate(t, 15, 1, 2027)}, DayCounter: dc}
	for _, bad := range []Callability{{-1, Clean, CallabilityCall, today}, {math.NaN(), Clean, CallabilityCall, today}, {math.Inf(1), Clean, CallabilityCall, today}, {100, BondPriceType(2), CallabilityCall, today}, {100, Clean, CallabilityType(2), today}, {100, Clean, CallabilityCall, Date{}}, {100, Clean, CallabilityCall, testDate(t, 16, 1, 2027)}} {
		if bad.Date.Serial() <= zero.MaturityDate.Serial() {
			if _, err := NewCallability(bad.Price, bad.PriceType, bad.Type, bad.Date); err == nil {
				t.Fatal("invalid value accepted")
			}
		}
		f, z := fixed, zero
		f.Callability, z.Callability = CallabilitySchedule{bad}, CallabilitySchedule{bad}
		if _, err := s.NewCallableFixedRateBond(f); err == nil {
			t.Fatal("invalid fixed schedule accepted")
		}
		if _, err := s.NewCallableZeroCouponBond(z); err == nil {
			t.Fatal("invalid zero schedule accepted")
		}
	}
	for _, mutate := range []func(*CallableFixedRateBondConfig){func(a *CallableFixedRateBondConfig) { a.Calendar = cal }, func(a *CallableFixedRateBondConfig) { a.FirstPeriodDayCounter = dc }, func(a *CallableFixedRateBondConfig) { a.AccrualDayCounter = nil }, func(a *CallableFixedRateBondConfig) { a.Schedule = nil }, func(a *CallableFixedRateBondConfig) { a.Coupons = nil }, func(a *CallableFixedRateBondConfig) { a.FaceAmount = 0 }, func(a *CallableFixedRateBondConfig) { a.SettlementDays = math.MaxUint32 }} {
		config := fixed
		mutate(&config)
		if _, err := s.NewCallableFixedRateBond(config); err == nil {
			t.Fatal("invalid fixed config accepted")
		}
	}
	zero.DayCounter = foreignDC
	if _, err := s.NewCallableZeroCouponBond(zero); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("cross-session day counter", err)
	}
	zero.DayCounter = dc
	redemption := 0.0
	zero.Redemption = &redemption
	z := pricingMust(s.NewCallableZeroCouponBond(zero))
	if len(pricingMust(z.Callability())) != 0 {
		t.Fatal("empty schedule")
	}
	leg := pricingMust(z.Redemptions())
	flow := pricingMust(leg.At(0))
	bondNear(t, pricingMust(flow.Amount()), 0)
	bondCheck(t, flow.Close())
	bondCheck(t, leg.Close())
	bondCheck(t, z.Close())
	f := pricingMust(s.NewCallableFixedRateBond(fixed))
	bondCheck(t, f.Close())
	if _, err := f.DayCounter(); err == nil {
		t.Fatal("closed fixed inspected")
	}
	bondCheck(t, schedule.Close())
	if _, err := s.NewCallableFixedRateBond(fixed); err == nil {
		t.Fatal("closed schedule accepted")
	}
}
func TestCallableBondFoundationConcurrentQueries(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	b, read, _, dc, _ := callableFoundationFixture(t, s, "fixed")
	want := pricingMust(read())
	var group sync.WaitGroup
	errors := make(chan error, 8)
	for i := 0; i < 8; i++ {
		group.Add(1)
		go func() {
			defer group.Done()
			for j := 0; j < 10; j++ {
				got, err := read()
				if err != nil {
					errors <- err
					return
				}
				if !reflect.DeepEqual(got, want) {
					errors <- &Error{Message: "schedule changed"}
					return
				}
				if _, err := b.CleanPriceFromYield(.047, BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Semiannual}); err != nil {
					errors <- err
					return
				}
			}
		}()
	}
	group.Wait()
	close(errors)
	for err := range errors {
		t.Fatal(err)
	}
}

func TestCallableBondFoundationCalendarMathAndNullDefault(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := testDate(t, 15, 1, 2024)
	maturity := testDate(t, 15, 1, 2045)
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(today))
	cal := pricingMust(s.Indonesia())
	dc := pricingMust(s.Actual365Fixed())
	unadjusted := Unadjusted
	zero := CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: BondConfig{FaceAmount: 1000, Settings: settings, Calendar: cal, PaymentConvention: &unadjusted}, MaturityDate: maturity}, DayCounter: dc}
	z := pricingMust(s.NewCallableZeroCouponBond(zero))
	if pricingMust(z.MaturityDate()) != maturity {
		t.Fatal("unadjusted maturity")
	}
	zero.PaymentConvention = nil
	if _, err := s.NewCallableZeroCouponBond(zero); err == nil {
		t.Fatal("holiday horizon was not guarded for adjustment")
	}
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: today, End: maturity, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted}))
	fixed := CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: BondConfig{FaceAmount: 1000, Settings: settings, PaymentConvention: &unadjusted}, Schedule: schedule, Coupons: []float64{.05}, AccrualDayCounter: dc, ExCouponPeriod: &Period{7, Days}}}
	f := pricingMust(s.NewCallableFixedRateBond(fixed))
	if pricingMust(f.MaturityDate()) != maturity {
		t.Fatal("fixed unadjusted maturity")
	}
	retained := pricingMust(f.Calendar())
	if _, err := retained.IsBusinessDay(maturity); err == nil {
		t.Fatal("calendar metadata lost")
	}
	fixed.ExCouponCalendar = cal
	if _, err := s.NewCallableFixedRateBond(fixed); err == nil {
		t.Fatal("business-day ex-coupon lookup was not guarded")
	}
	fixed.ExCouponPeriod, fixed.ExCouponEndOfMonth = &Period{1, Months}, true
	monthEx := pricingMust(s.NewCallableFixedRateBond(fixed))
	bondCheck(t, monthEx.Close())
	fixed.ExCouponCalendar, fixed.ExCouponPeriod, fixed.ExCouponEndOfMonth = nil, &Period{7, Days}, false
	nullCalendar := pricingMust(s.NullCalendar())
	fixed.ExCouponCalendar = nullCalendar
	explicit := pricingMust(s.NewCallableFixedRateBond(fixed))
	settlement := testDate(t, 9, 7, 2044)
	defaultAccrued := pricingMust(f.AccruedAmount(&settlement))
	if defaultAccrued >= 0 {
		t.Fatal("default NullCalendar ex-coupon window")
	}
	bondNear(t, defaultAccrued, pricingMust(explicit.AccruedAmount(&settlement)))
	yieldConfig := BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	bondNear(t, pricingMust(f.CleanPriceFromYield(.04, yieldConfig)), pricingMust(explicit.CleanPriceFromYield(.04, yieldConfig)))
	defaultLeg, explicitLeg := pricingMust(f.Cashflows()), pricingMust(explicit.Cashflows())
	if pricingMust(defaultLeg.Len()) != pricingMust(explicitLeg.Len()) {
		t.Fatal("default NullCalendar cashflows")
	}
	for i := 0; i < pricingMust(defaultLeg.Len()); i++ {
		defaultFlow, explicitFlow := pricingMust(defaultLeg.At(i)), pricingMust(explicitLeg.At(i))
		if pricingMust(defaultFlow.Date()) != pricingMust(explicitFlow.Date()) {
			t.Fatal("default NullCalendar payment date")
		}
		bondNear(t, pricingMust(defaultFlow.Amount()), pricingMust(explicitFlow.Amount()))
		bondCheck(t, defaultFlow.Close())
		bondCheck(t, explicitFlow.Close())
	}
	bondCheck(t, cal.Close())
	bondCheck(t, schedule.Close())
	bondCheck(t, retained.Close())
	bondCheck(t, defaultLeg.Close())
	bondCheck(t, explicitLeg.Close())
}

func TestCallableBondFoundationNativeOnceSchedule(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := testDate(t, 15, 1, 2024)
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(today))
	cal := pricingMust(s.NullCalendar())
	dc := pricingMust(s.Actual365Fixed())
	zeroRule := Zero
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: today, End: testDate(t, 15, 1, 2027), Frequency: Annual, Calendar: cal, Convention: Unadjusted, Rule: &zeroRule}))
	config := FixedRateBondConfig{BondConfig: BondConfig{FaceAmount: 1000, Settings: settings, SettlementDays: 2}, Schedule: schedule, Coupons: []float64{.05}, AccrualDayCounter: dc}
	callable := pricingMust(s.NewCallableFixedRateBond(CallableFixedRateBondConfig{FixedRateBondConfig: config}))
	plain := pricingMust(s.NewFixedRateBond(config))
	if _, err := callable.Frequency(); err == nil {
		t.Fatal("unsupported native frequency ordinal was fabricated")
	}
	yieldConfig := BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Semiannual}
	bondNear(t, pricingMust(callable.CleanPriceFromYield(.04, yieldConfig)), pricingMust(plain.CleanPriceFromYield(.04, yieldConfig)))
	if pricingMust(callable.IsCalculated()) {
		t.Fatal("stateless analytics changed callable cache")
	}
	bondCheck(t, schedule.Close())
	if pricingMust(callable.MaturityDate()) != testDate(t, 15, 1, 2027) {
		t.Fatal("retained zero-rule schedule")
	}
}
