package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"sync"
	"testing"
)

type analyticsCase struct {
	Name, Kind                        string
	Compounding                       Compounding
	Frequency                         Frequency
	Face                              float64
	Start, End, Reference, Settlement int32
	NPVDate                           int32 `json:"npv_date"`
	Yield, Spread                     float64
	Expected                          map[string]float64
}

func analyticsOracles(t *testing.T) []analyticsCase {
	t.Helper()
	data, err := os.ReadFile("testdata/bond_analytics_oracle.json")
	bondCheck(t, err)
	var x struct {
		Version string `json:"quantlib_version"`
		Cases   []analyticsCase
	}
	bondCheck(t, json.Unmarshal(data, &x))
	if x.Version != "1.43" || len(x.Cases) != 55 {
		t.Fatal("oracle contract")
	}
	return x.Cases
}
func analyticsNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > math.Max(3e-12, 3e-13*math.Abs(want)) {
		t.Fatalf("got %.17g want %.17g", got, want)
	}
}
func analyticsSolveNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > 5e-12 {
		t.Fatalf("solve got %.17g want %.17g", got, want)
	}
}
func analyticsFixture(t *testing.T, s *Session, a analyticsCase) (*Bond, *Settings, *DayCounter, *YieldTermStructure) {
	t.Helper()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual360()
	bondCheck(t, e)
	curveDC, e := s.Actual365Fixed()
	bondCheck(t, e)
	reference, e := DateFromSerial(a.Reference)
	bondCheck(t, e)
	curve, e := s.NewFlatForward(reference, .032, curveDC)
	bondCheck(t, e)
	start, e := DateFromSerial(a.Start)
	bondCheck(t, e)
	end, e := DateFromSerial(a.End)
	bondCheck(t, e)
	redemption := 102.0
	convention := Unadjusted
	config := BondConfig{SettlementDays: 5, FaceAmount: a.Face, Settings: settings, IssueDate: &start, PaymentConvention: &convention, Redemption: &redemption}
	var b *Bond
	if a.Kind == "zero" {
		config.Calendar = cal
		zero, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: config, MaturityDate: end})
		bondCheck(t, e)
		b = &zero.Bond
	} else {
		backward := Backward
		schedule, e := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &backward})
		bondCheck(t, e)
		fixedConfig := FixedRateBondConfig{BondConfig: config, Schedule: schedule, Coupons: []float64{.047, .058}, AccrualDayCounter: dc}
		if a.Kind == "ex_coupon" {
			fixedConfig.ExCouponPeriod = &Period{7, Days}
			fixedConfig.ExCouponCalendar = cal
		}
		fixed, e := s.NewFixedRateBond(fixedConfig)
		bondCheck(t, e)
		b = &fixed.Bond
		bondCheck(t, schedule.Close())
	}
	bondCheck(t, cal.Close())
	bondCheck(t, curveDC.Close())
	return b, settings, dc, curve
}
func TestBondAnalyticsQuantLibOracles(t *testing.T) {
	for _, a := range analyticsOracles(t) {
		t.Run(a.Name, func(t *testing.T) {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			b, settings, dc, curve := analyticsFixture(t, s, a)
			settlement, e := DateFromSerial(a.Settlement)
			bondCheck(t, e)
			npvDate, e := DateFromSerial(a.NPVDate)
			bondCheck(t, e)
			y := BondYieldConfig{DayCounter: dc, Compounding: a.Compounding, Frequency: a.Frequency, SettlementDate: &settlement}
			z := BondZSpreadConfig{Discount: curve, Compounding: a.Compounding, Frequency: a.Frequency, SettlementDate: &settlement}
			lc := CashFlowsZSpreadConfig{CashFlowsNPVConfig: CashFlowsNPVConfig{Discount: curve, Settings: settings, SettlementDate: &settlement, NPVDate: &npvDate}, Compounding: a.Compounding, Frequency: a.Frequency}
			leg, e := b.Cashflows()
			bondCheck(t, e)
			defer leg.Close()
			checks := []struct {
				name string
				call func() (float64, error)
			}{
				{"clean_curve", func() (float64, error) { return b.CleanPriceFromCurve(curve, &settlement) }},
				{"dirty_curve", func() (float64, error) { return b.DirtyPriceFromCurve(curve, &settlement) }},
				{"clean_yield", func() (float64, error) { return b.CleanPriceFromYield(a.Yield, y) }},
				{"dirty_yield", func() (float64, error) { return b.DirtyPriceFromYield(a.Yield, y) }},
				{"clean_spread", func() (float64, error) { return b.CleanPriceAtZSpread(a.Spread, z) }},
				{"dirty_spread", func() (float64, error) { return b.DirtyPriceAtZSpread(a.Spread, z) }},
				{"leg_npv", func() (float64, error) { return leg.NPVAtZSpread(a.Spread, lc) }},
			}
			for _, check := range checks {
				v, e := check.call()
				bondCheck(t, e)
				analyticsNear(t, v, a.Expected[check.name])
			}
			accuracy := 1e-12
			evaluations := uint(1000)
			solver := AnalyticsSolverConfig{Accuracy: &accuracy, MaxEvaluations: &evaluations}
			for _, priceType := range []BondPriceType{Clean, Dirty} {
				kind := "clean"
				if priceType == Dirty {
					kind = "dirty"
				}
				v, e := b.YieldRate(a.Expected[kind+"_yield"], BondYieldSolveConfig{BondYieldConfig: y, PriceType: priceType, AnalyticsSolverConfig: solver})
				bondCheck(t, e)
				analyticsSolveNear(t, v, a.Expected["yield_"+kind])
				v, e = b.ZSpread(a.Expected[kind+"_spread"], BondZSpreadSolveConfig{BondZSpreadConfig: z, PriceType: priceType, AnalyticsSolverConfig: solver})
				bondCheck(t, e)
				analyticsSolveNear(t, v, a.Expected["spread_"+kind])
			}
			v, e := leg.ZSpread(a.Expected["leg_npv"], CashFlowsZSpreadSolveConfig{CashFlowsZSpreadConfig: lc, AnalyticsSolverConfig: solver})
			bondCheck(t, e)
			analyticsSolveNear(t, v, a.Expected["leg_spread"])
			calculated, e := b.IsCalculated()
			bondCheck(t, e)
			if calculated {
				t.Fatal("pure analytics calculated engine")
			}
			if _, e := b.CleanPrice(); e == nil {
				t.Fatal("engine pricing succeeded without evaluation date")
			}
			if _, e := b.CleanPriceFromCurve(curve, nil); e == nil {
				t.Fatal("default settlement succeeded without evaluation date")
			}
		})
	}
}

func TestBondAnalyticsSolverDefaultsErrorsAndRecovery(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	a := analyticsOracles(t)[5]
	b, settings, dc, curve := analyticsFixture(t, s, a)
	settlement, e := DateFromSerial(a.Settlement)
	bondCheck(t, e)
	y := BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	z := BondZSpreadConfig{Discount: curve, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	target, e := b.CleanPriceFromYield(.052, y)
	bondCheck(t, e)
	rate, e := b.YieldRate(target, BondYieldSolveConfig{BondYieldConfig: y})
	bondCheck(t, e)
	if math.Abs(rate-.052) > 2e-9 {
		t.Fatal("default yield solve")
	}
	spreadTarget, e := b.CleanPriceAtZSpread(.007, z)
	bondCheck(t, e)
	spread, e := b.ZSpread(spreadTarget, BondZSpreadSolveConfig{BondZSpreadConfig: z})
	bondCheck(t, e)
	if math.Abs(spread-.007) > 2e-9 {
		t.Fatal("default spread solve")
	}
	zero := 0.0
	zeroCount := uint(0)
	one := uint(1)
	negative := -1.0
	inf := math.Inf(1)
	solvers := []AnalyticsSolverConfig{{Accuracy: &zero}, {Accuracy: &negative}, {Accuracy: &inf}, {MaxEvaluations: &zeroCount}, {MaxEvaluations: &one}, {Guess: &inf}}
	for _, solve := range solvers {
		if _, e := b.YieldRate(target, BondYieldSolveConfig{BondYieldConfig: y, AnalyticsSolverConfig: solve}); e == nil {
			t.Fatal("invalid yield solver accepted")
		}
		if _, e := b.ZSpread(spreadTarget, BondZSpreadSolveConfig{BondZSpreadConfig: z, AnalyticsSolverConfig: solve}); e == nil {
			t.Fatal("invalid spread solver accepted")
		}
	}
	if _, e = b.YieldRate(target, BondYieldSolveConfig{BondYieldConfig: y, AnalyticsSolverConfig: AnalyticsSolverConfig{Guess: &zero}}); e == nil {
		t.Fatal("zero guess silently replaced by default")
	}
	calendar, e := s.NullCalendar()
	bondCheck(t, e)
	zeroBond, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{SettlementDays: 2, FaceAmount: 250, Settings: settings, Calendar: calendar}, MaturityDate: testDate(t, 26, 6, 2027)})
	bondCheck(t, e)
	zeroSettlement := testDate(t, 26, 6, 2026)
	zeroConfig := y
	zeroConfig.SettlementDate = &zeroSettlement
	rate, e = zeroBond.YieldRate(100, BondYieldSolveConfig{BondYieldConfig: zeroConfig, PriceType: Dirty, AnalyticsSolverConfig: AnalyticsSolverConfig{Guess: &zero}})
	bondCheck(t, e)
	analyticsNear(t, rate, 0)
	rate, e = b.YieldRate(target, BondYieldSolveConfig{BondYieldConfig: y})
	bondCheck(t, e)
	if math.Abs(rate-.052) > 2e-9 {
		t.Fatal("solver did not recover")
	}
	invalid := []BondYieldConfig{y, y, y, y, y}
	invalid[0].Compounding = -1
	invalid[1].Frequency = Frequency(4)
	invalid[2].SettlementDate = &Date{}
	invalid[3].DayCounter = nil
	invalid[4].DayCounter = &DayCounter{curve.object}
	for _, cfg := range invalid {
		if _, e := b.CleanPriceFromYield(.052, cfg); e == nil {
			t.Fatal("invalid conventions/date/handle accepted")
		}
	}
	for _, value := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, e := b.DirtyPriceFromYield(value, y); e == nil {
			t.Fatal("nonfinite yield accepted")
		}
		if _, e := b.DirtyPriceAtZSpread(value, z); e == nil {
			t.Fatal("nonfinite spread accepted")
		}
		if _, e := b.YieldRate(value, BondYieldSolveConfig{BondYieldConfig: y}); e == nil {
			t.Fatal("nonfinite target accepted")
		}
	}
	if _, e := b.YieldRate(target, BondYieldSolveConfig{BondYieldConfig: y, PriceType: BondPriceType(2)}); e == nil {
		t.Fatal("price type accepted")
	}
	for _, mode := range []Compounding{Simple, Compounded, SimpleThenCompounded, CompoundedThenSimple} {
		cfg := y
		cfg.Compounding = mode
		if _, e := b.DirtyPriceFromYield(-100, cfg); e == nil {
			t.Fatal("invalid negative compounding domain accepted")
		}
	}
	mature, e := DateFromSerial(a.End)
	bondCheck(t, e)
	if _, e := b.CleanPriceFromCurve(curve, &mature); e == nil {
		t.Fatal("nontradable analytics succeeded")
	}
	wrong := Bond{curve.object}
	if _, e := wrong.DirtyPriceFromCurve(curve, &settlement); e == nil {
		t.Fatal("wrong bond type accepted")
	}
	other, e := NewSession()
	bondCheck(t, e)
	defer other.Close()
	otherDC, e := other.Actual360()
	bondCheck(t, e)
	cfg := y
	cfg.DayCounter = otherDC
	if _, e := b.DirtyPriceFromYield(.052, cfg); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("cross session error %v", e)
	}
	bondCheck(t, settings.Close())
	_, e = b.CleanPriceFromCurve(curve, &settlement)
	bondCheck(t, e)
	bondCheck(t, dc.Close())
	if _, e := b.DirtyPriceFromYield(.052, y); e == nil {
		t.Fatal("released day counter accepted")
	}
	_, e = b.DirtyPriceFromCurve(curve, &settlement)
	bondCheck(t, e)
}
func TestBondAnalyticsOmittedSettlementAndExplicitDatePurity(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	a := analyticsOracles(t)[0]
	b, settings, dc, curve := analyticsFixture(t, s, a)
	today := testDate(t, 19, 6, 2026)
	bondCheck(t, settings.SetEvaluationDate(today))
	actual, e := b.SettlementDate(nil)
	bondCheck(t, e)
	expected := testDate(t, 24, 6, 2026)
	if actual.Serial() != expected.Serial() {
		t.Fatal("default settlement")
	}
	omitted, e := b.CleanPriceFromCurve(curve, nil)
	bondCheck(t, e)
	explicit, e := b.CleanPriceFromCurve(curve, &actual)
	bondCheck(t, e)
	analyticsNear(t, omitted, explicit)
	distinct := testDate(t, 26, 6, 2026)
	direct, e := b.DirtyPriceFromCurve(curve, &distinct)
	bondCheck(t, e)
	analyticsNear(t, direct, a.Expected["dirty_curve"])
	if _, e := b.YieldRate(100, BondYieldSolveConfig{BondYieldConfig: BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Semiannual}}); e != nil {
		bondCheck(t, e)
	}
}
func TestBondAnalyticsFrozenFloatingCurrentInputsAndRetainedLeg(t *testing.T) {
	data, e := os.ReadFile("testdata/bond_analytics_oracle.json")
	bondCheck(t, e)
	var oracle struct {
		Floating struct {
			Initial, Updated map[string]float64
			Cached           float64 `json:"cached_npv"`
			Settlement       int32
		}
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, floatingOracles(t)[5], true)
	cached, e := m.bond.NPV()
	bondCheck(t, e)
	analyticsNear(t, cached, oracle.Floating.Cached)
	bondCheck(t, m.bond.Freeze())
	settlement, e := DateFromSerial(oracle.Floating.Settlement)
	bondCheck(t, e)
	analyticsCurve, e := s.NewFlatForward(testDate(t, 15, 1, 2024), .051, m.counter)
	bondCheck(t, e)
	y := BondYieldConfig{DayCounter: m.counter, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	z := BondZSpreadConfig{Discount: analyticsCurve, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	leg, e := m.bond.Cashflows()
	bondCheck(t, e)
	otherSettings, e := s.NewSettings()
	bondCheck(t, e)
	lc := CashFlowsZSpreadConfig{CashFlowsNPVConfig: CashFlowsNPVConfig{Discount: analyticsCurve, Settings: otherSettings, SettlementDate: &settlement}, Compounding: Compounded, Frequency: Semiannual}
	check := func(w map[string]float64) {
		t.Helper()
		beforeCalculated, e := m.bond.IsCalculated()
		bondCheck(t, e)
		checks := []struct {
			name string
			call func() (float64, error)
		}{
			{"clean_curve", func() (float64, error) { return m.bond.CleanPriceFromCurve(analyticsCurve, &settlement) }},
			{"dirty_curve", func() (float64, error) { return m.bond.DirtyPriceFromCurve(analyticsCurve, &settlement) }},
			{"clean_yield", func() (float64, error) { return m.bond.CleanPriceFromYield(.047, y) }},
			{"dirty_yield", func() (float64, error) { return m.bond.DirtyPriceFromYield(.047, y) }},
			{"clean_spread", func() (float64, error) { return m.bond.CleanPriceAtZSpread(.006, z) }},
			{"dirty_spread", func() (float64, error) { return m.bond.DirtyPriceAtZSpread(.006, z) }},
			{"leg_npv", func() (float64, error) { return leg.NPVAtZSpread(.006, lc) }},
		}
		for _, item := range checks {
			v, e := item.call()
			bondCheck(t, e)
			analyticsNear(t, v, w[item.name])
		}
		value, e := m.bond.YieldRate(w["clean_yield"], BondYieldSolveConfig{BondYieldConfig: y})
		bondCheck(t, e)
		if math.Abs(value-.047) > 2e-9 {
			t.Fatal("frozen yield inversion")
		}
		value, e = m.bond.ZSpread(w["clean_spread"], BondZSpreadSolveConfig{BondZSpreadConfig: z})
		bondCheck(t, e)
		if math.Abs(value-.006) > 2e-9 {
			t.Fatal("frozen spread inversion")
		}
		value, e = leg.ZSpread(w["leg_npv"], CashFlowsZSpreadSolveConfig{CashFlowsZSpreadConfig: lc})
		bondCheck(t, e)
		if math.Abs(value-.006) > 2e-9 {
			t.Fatal("frozen leg inversion")
		}
		value, e = m.bond.NPV()
		bondCheck(t, e)
		analyticsNear(t, value, cached)
		calculated, e := m.bond.IsCalculated()
		bondCheck(t, e)
		if calculated != beforeCalculated {
			t.Fatal("analytical method changed cache state")
		}
	}
	check(oracle.Floating.Initial)
	bondCheck(t, m.rate.SetValue(.037))
	bondCheck(t, m.volatility.SetValue(.3))
	check(oracle.Floating.Updated)
	badSurface, e := s.ConstantOptionletVolatility(ConstantRateVolConfig{Settings: m.settings, SettlementDays: 100000, Calendar: m.calendar, Convention: Following, Volatility: .2, DayCounter: m.counter})
	bondCheck(t, e)
	bad, e := s.NewBlackIborCouponPricer(badSurface)
	bondCheck(t, e)
	bondCheck(t, m.bond.SetCouponPricer(bad))
	if _, e := m.bond.DirtyPriceFromCurve(analyticsCurve, &settlement); e == nil {
		t.Fatal("frozen analytics skipped required volatility guard")
	}
	if _, e := leg.NPVAtZSpread(.006, lc); e == nil {
		t.Fatal("retained leg skipped required volatility guard")
	}
	still, e := m.bond.NPV()
	bondCheck(t, e)
	analyticsNear(t, still, cached)
	bondCheck(t, m.bond.SetCouponPricer(m.pricer))
	check(oracle.Floating.Updated)
	for _, close := range []func() error{m.index.Close, m.calendar.Close, m.pricer.Close, m.surface.Close, m.rate.Close, m.volatility.Close, m.bond.Close} {
		bondCheck(t, close())
	}
	value, e := leg.NPVAtZSpread(.006, lc)
	bondCheck(t, e)
	analyticsNear(t, value, oracle.Floating.Updated["leg_npv"])
	bondCheck(t, leg.Close())
}
func TestBondAnalyticsConcurrentSessionClose(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	a := analyticsOracles(t)[0]
	b, _, dc, _ := analyticsFixture(t, s, a)
	settlement, e := DateFromSerial(a.Settlement)
	bondCheck(t, e)
	cfg := BondYieldConfig{DayCounter: dc, Compounding: Simple, Frequency: Annual, SettlementDate: &settlement}
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for range 20 {
				v, e := b.DirtyPriceFromYield(a.Yield, cfg)
				if e != nil && !errors.Is(e, ErrClosed) {
					t.Errorf("unexpected close error: %v", e)
				}
				if e == nil && (math.IsNaN(v) || math.IsInf(v, 0)) {
					t.Error("nonfinite concurrent price")
				}
			}
		}()
	}
	bondCheck(t, s.Close())
	wg.Wait()
}

func TestBondAnalyticsLegTriStateInclusion(t *testing.T) {
	data, e := os.ReadFile("testdata/bond_analytics_oracle.json")
	bondCheck(t, e)
	var oracle struct {
		Inclusion []struct {
			Today, Include *bool
			Reference      bool
			NPV, Spread    float64
		}
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	if len(oracle.Inclusion) != 18 {
		t.Fatal("tri-state fixture")
	}
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	a := analyticsOracles(t)[25]
	a.Face = 375
	b, settings, _, curve := analyticsFixture(t, s, a)
	leg, e := b.Cashflows()
	bondCheck(t, e)
	defer leg.Close()
	settlement, e := DateFromSerial(a.End)
	bondCheck(t, e)
	npvDate, e := DateFromSerial(a.End - 9)
	bondCheck(t, e)
	bondCheck(t, settings.SetEvaluationDate(settlement))
	accuracy := 1e-12
	count := uint(1000)
	for _, row := range oracle.Inclusion {
		bondCheck(t, settings.SetIncludeTodaysCashFlows(row.Today))
		bondCheck(t, settings.SetIncludeReferenceDateEvents(row.Reference))
		config := CashFlowsZSpreadConfig{CashFlowsNPVConfig: CashFlowsNPVConfig{Discount: curve, Settings: settings, SettlementDate: &settlement, NPVDate: &npvDate, IncludeSettlementDateFlows: row.Include}, Compounding: Compounded, Frequency: Semiannual}
		v, e := leg.NPVAtZSpread(.007, config)
		bondCheck(t, e)
		analyticsNear(t, v, row.NPV)
		v, e = leg.ZSpread(row.NPV, CashFlowsZSpreadSolveConfig{CashFlowsZSpreadConfig: config, AnalyticsSolverConfig: AnalyticsSolverConfig{Accuracy: &accuracy, MaxEvaluations: &count}})
		bondCheck(t, e)
		analyticsSolveNear(t, v, row.Spread)
	}
}
func TestBondAnalyticsFloatingSelectedFlowAndCleanAccrualGuards(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, floatingOracles(t)[0], true)
	cap := .032
	config := m.config
	config.Caps = []*float64{&cap, nil}
	config.ExCouponPeriod = &Period{190, Days}
	config.ExCouponCalendar = m.calendar
	b, e := s.NewFloatingRateBond(config)
	bondCheck(t, e)
	surface, e := s.ConstantOptionletVolatility(ConstantRateVolConfig{Settings: m.settings, SettlementDays: 100000, Calendar: m.calendar, Convention: Following, Volatility: .2, DayCounter: m.counter})
	bondCheck(t, e)
	pricer, e := s.NewBlackIborCouponPricer(surface)
	bondCheck(t, e)
	bondCheck(t, b.SetCouponPricer(pricer))
	bondCheck(t, m.settings.SetEvaluationDate(testDate(t, 11, 7, 2024)))
	settlement := testDate(t, 13, 7, 2024)
	leg, e := b.Cashflows()
	bondCheck(t, e)
	defer leg.Close()
	z := BondZSpreadConfig{Discount: m.discountCurve, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	y := BondYieldConfig{DayCounter: m.counter, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	for _, read := range []func() (float64, error){func() (float64, error) { return b.DirtyPriceFromCurve(m.discountCurve, &settlement) }, func() (float64, error) { return b.CleanPriceFromCurve(m.discountCurve, &settlement) }, func() (float64, error) { return b.DirtyPriceAtZSpread(.006, z) }, func() (float64, error) { return b.DirtyPriceFromYield(.047, y) }} {
		_, e := read()
		bondCheck(t, e)
	}
	other, e := s.NewSettings()
	bondCheck(t, e)
	lc := CashFlowsZSpreadConfig{CashFlowsNPVConfig: CashFlowsNPVConfig{Discount: m.discountCurve, Settings: other, SettlementDate: &settlement}, Compounding: Compounded, Frequency: Semiannual}
	_, e = leg.NPVAtZSpread(.006, lc)
	bondCheck(t, e)
	bondCheck(t, m.settings.SetEvaluationDate(testDate(t, 17, 7, 2024)))
	settlement = testDate(t, 19, 7, 2024)
	_, e = b.DirtyPriceFromCurve(m.discountCurve, &settlement)
	bondCheck(t, e)
	dirty, e := b.DirtyPriceFromYield(.047, y)
	bondCheck(t, e)
	_, e = b.YieldRate(dirty, BondYieldSolveConfig{BondYieldConfig: y, PriceType: Dirty})
	bondCheck(t, e)
	spreadDirty, e := b.DirtyPriceAtZSpread(.006, z)
	bondCheck(t, e)
	_, e = b.ZSpread(spreadDirty, BondZSpreadSolveConfig{BondZSpreadConfig: z, PriceType: Dirty})
	bondCheck(t, e)
	for _, read := range []func() (float64, error){func() (float64, error) { return b.CleanPriceFromCurve(m.discountCurve, &settlement) }, func() (float64, error) { return b.CleanPriceFromYield(.047, y) }, func() (float64, error) { return b.CleanPriceAtZSpread(.006, z) }, func() (float64, error) { return b.YieldRate(dirty, BondYieldSolveConfig{BondYieldConfig: y}) }, func() (float64, error) { return b.ZSpread(spreadDirty, BondZSpreadSolveConfig{BondZSpreadConfig: z}) }} {
		if _, e := read(); e == nil {
			t.Fatal("clean analytics skipped required excluded-coupon accrual")
		}
	}
	maturity := testDate(t, 16, 7, 2026)
	lc.SettlementDate = &maturity
	v, e := leg.NPVAtZSpread(.006, lc)
	bondCheck(t, e)
	analyticsNear(t, v, 0)
}
