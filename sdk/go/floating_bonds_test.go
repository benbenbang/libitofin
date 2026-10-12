package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"sync"
	"testing"
)

type floatingSnapshot struct {
	NPV             float64   `json:"npv"`
	SettlementValue float64   `json:"settlement_value"`
	Clean           float64   `json:"clean"`
	Dirty           float64   `json:"dirty"`
	Accrued         float64   `json:"accrued"`
	Cashflows       []float64 `json:"cashflows"`
}
type floatingCase struct {
	Name              string `json:"name"`
	Today, Start, End int32
	Gearings, Spreads []float64
	Caps, Floors      []*float64
	Indexed           bool
	FixingDays        *uint32 `json:"fixing_days"`
	ExCoupon          bool    `json:"ex_coupon"`
	Historical        bool
	Normal            bool
	Initial, Updated  floatingSnapshot
}
type floatingMarket struct {
	bond                       *FloatingRateBond
	settings                   *Settings
	calendar                   *Calendar
	counter, indexCounter      *DayCounter
	schedule                   *Schedule
	index                      *IborIndex
	rate, discount, volatility *SimpleQuote
	forward, discountCurve     *YieldTermStructure
	surface                    *OptionletVolatilityStructure
	pricer                     *BlackIborCouponPricer
	engine                     *DiscountingBondEngine
	config                     FloatingRateBondConfig
}

func floatingOracles(t *testing.T) []floatingCase {
	t.Helper()
	data, e := os.ReadFile("testdata/floating_bonds_oracle.json")
	bondCheck(t, e)
	var x struct {
		Version string `json:"quantlib_version"`
		Cases   []floatingCase
	}
	bondCheck(t, json.Unmarshal(data, &x))
	if x.Version != "1.43" {
		t.Fatal("oracle version")
	}
	return x.Cases
}
func floatingFixture(t *testing.T, s *Session, c floatingCase, attach bool) floatingMarket {
	t.Helper()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	today, e := DateFromSerial(c.Today)
	bondCheck(t, e)
	bondCheck(t, settings.SetEvaluationDate(today))
	bondCheck(t, settings.SetUsingAtParCoupons(!c.Indexed))
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	couponDC, e := s.Actual360()
	bondCheck(t, e)
	currency, e := s.USD()
	bondCheck(t, e)
	reference := testDate(t, 15, 1, 2024)
	rate, e := s.NewSimpleQuote(.03)
	bondCheck(t, e)
	discount, e := s.NewSimpleQuote(.04)
	bondCheck(t, e)
	forward, e := s.NewFlatForwardFromQuote(reference, rate, dc)
	bondCheck(t, e)
	discountCurve, e := s.NewFlatForwardFromQuote(reference, discount, dc)
	bondCheck(t, e)
	index, e := s.NewIborIndex(IborIndexConfig{FamilyName: "BondBindingOracle", Tenor: Period{6, Months}, SettlementDays: 2, Currency: currency, FixingCalendar: cal, Convention: ModifiedFollowing, DayCounter: couponDC, Forwarding: forward, Settings: settings})
	bondCheck(t, e)
	bondCheck(t, currency.Close())
	start, e := DateFromSerial(c.Start)
	bondCheck(t, e)
	end, e := DateFromSerial(c.End)
	bondCheck(t, e)
	backward := Backward
	schedule, e := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &backward})
	bondCheck(t, e)
	config := FloatingRateBondConfig{SettlementDays: 2, FaceAmount: 1000, Schedule: schedule, Index: index, AccrualDayCounter: couponDC, Settings: settings, FixingDays: c.FixingDays, Gearings: c.Gearings, Spreads: c.Spreads, Caps: c.Caps, Floors: c.Floors, IssueDate: &start}
	if c.ExCoupon {
		config.ExCouponPeriod = &Period{7, Days}
		config.ExCouponCalendar = cal
	}
	bond, e := s.NewFloatingRateBond(config)
	bondCheck(t, e)
	vol := .2
	kind := ShiftedLognormal
	if c.Normal {
		vol = .01
		kind = Normal
	}
	volatility, e := s.NewSimpleQuote(vol)
	bondCheck(t, e)
	surface, e := s.ConstantOptionletVolatility(ConstantRateVolConfig{ReferenceDate: reference, Calendar: cal, Convention: Following, Quote: volatility, DayCounter: dc, VolatilityType: kind})
	bondCheck(t, e)
	pricer, e := s.NewBlackIborCouponPricer(surface)
	bondCheck(t, e)
	engine, e := s.NewDiscountingBondEngine(discountCurve, settings, nil)
	bondCheck(t, e)
	bondCheck(t, bond.SetEngine(engine))
	if attach {
		bondCheck(t, bond.SetCouponPricer(pricer))
	}
	return floatingMarket{bond, settings, cal, dc, couponDC, schedule, index, rate, discount, volatility, forward, discountCurve, surface, pricer, engine, config}
}
func floatingSnapshotCheck(t *testing.T, b *FloatingRateBond, w floatingSnapshot) {
	t.Helper()
	checks := []struct {
		read func() (float64, error)
		want float64
	}{{b.NPV, w.NPV}, {b.SettlementValue, w.SettlementValue}, {b.CleanPrice, w.Clean}, {b.DirtyPrice, w.Dirty}, {func() (float64, error) { return b.AccruedAmount(nil) }, w.Accrued}}
	for _, check := range checks {
		v, e := check.read()
		bondCheck(t, e)
		bondNear(t, v, check.want)
	}
	leg, e := b.Cashflows()
	bondCheck(t, e)
	defer leg.Close()
	n, e := leg.Len()
	bondCheck(t, e)
	if n != len(w.Cashflows) {
		t.Fatal("cashflow count")
	}
	for i, want := range w.Cashflows {
		flow, e := leg.At(i)
		bondCheck(t, e)
		v, e := flow.Amount()
		bondCheck(t, e)
		bondNear(t, v, want)
		bondCheck(t, flow.Close())
	}
}
func TestFloatingBondQuantLibOracles(t *testing.T) {
	for _, c := range floatingOracles(t) {
		t.Run(c.Name, func(t *testing.T) {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			m := floatingFixture(t, s, c, true)
			if c.Historical || c.ExCoupon {
				if _, e := m.bond.NPV(); e == nil {
					t.Fatal("missing history priced")
				}
				bondCheck(t, m.index.AddFixing(testDate(t, 13, 7, 2024), .0275))
			}
			floatingSnapshotCheck(t, m.bond, c.Initial)
			bondCheck(t, m.rate.SetValue(.037))
			bondCheck(t, m.discount.SetValue(.045))
			vol := .3
			if c.Normal {
				vol = .015
			}
			bondCheck(t, m.volatility.SetValue(vol))
			floatingSnapshotCheck(t, m.bond, c.Updated)
			if c.ExCoupon {
				accrued, e := m.bond.AccruedAmount(nil)
				bondCheck(t, e)
				if accrued >= 0 {
					t.Fatal("ex-coupon accrued not negative")
				}
			}
		})
	}
}
func TestFloatingBondRetainedViewsPricerReplacementAndFreeze(t *testing.T) {
	c := floatingOracles(t)[5]
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, c, false)
	leg, e := m.bond.Cashflows()
	bondCheck(t, e)
	defer leg.Close()
	flow, e := leg.At(0)
	bondCheck(t, e)
	defer flow.Close()
	if _, e := flow.Amount(); e == nil {
		t.Fatal("cap priced without pricer")
	}
	noVol, e := s.NewBlackIborCouponPricer(nil)
	bondCheck(t, e)
	bondCheck(t, m.bond.SetCouponPricer(noVol))
	if _, e := flow.Amount(); e == nil {
		t.Fatal("forecast cap priced without volatility")
	}
	bondCheck(t, m.bond.SetCouponPricer(m.pricer))
	v, e := flow.Amount()
	bondCheck(t, e)
	bondNear(t, v, c.Initial.Cashflows[0])
	inspection, e := m.bond.Index()
	bondCheck(t, e)
	oldName, e := m.index.Name()
	bondCheck(t, e)
	name, e := inspection.Name()
	bondCheck(t, e)
	if name != oldName {
		t.Fatal("wrong retained index")
	}
	for _, close := range []func() error{m.calendar.Close, m.counter.Close, m.indexCounter.Close, m.schedule.Close, m.index.Close, m.forward.Close, m.discountCurve.Close, m.surface.Close, m.pricer.Close, m.engine.Close, noVol.Close} {
		bondCheck(t, close())
	}
	old, e := m.bond.NPV()
	bondCheck(t, e)
	bondCheck(t, m.bond.Freeze())
	bondCheck(t, m.rate.SetValue(.037))
	bondCheck(t, m.discount.SetValue(.045))
	bondCheck(t, m.volatility.SetValue(.3))
	v, e = m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, old)
	v, e = flow.Amount()
	bondCheck(t, e)
	bondNear(t, v, c.Updated.Cashflows[0])
	bondCheck(t, m.bond.Unfreeze())
	v, e = m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, c.Updated.NPV)
	bondCheck(t, m.bond.Close())
	v, e = flow.Amount()
	bondCheck(t, e)
	bondNear(t, v, c.Updated.Cashflows[0])
	name, e = inspection.Name()
	bondCheck(t, e)
	if name != oldName {
		t.Fatal("index lost after bond close")
	}
	bondCheck(t, inspection.Close())
	if _, e := m.bond.Index(); e == nil {
		t.Fatal("released bond inspected")
	}
}
func TestFloatingBondAtParPolicyAndInputErrors(t *testing.T) {
	cases := floatingOracles(t)
	atPar, indexed := cases[11], cases[12]
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, atPar, true)
	v, e := m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, atPar.Initial.NPV)
	bondCheck(t, m.bond.Freeze())
	bondCheck(t, m.settings.SetUsingAtParCoupons(false))
	v, e = m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, atPar.Initial.NPV)
	bondCheck(t, m.bond.Unfreeze())
	v, e = m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, indexed.Initial.NPV)
	bondCheck(t, m.settings.SetUsingAtParCoupons(true))
	v, e = m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, atPar.Initial.NPV)
	configs := []FloatingRateBondConfig{m.config, m.config, m.config, m.config, m.config}
	configs[0].Gearings = []float64{math.NaN()}
	configs[1].Spreads = []float64{math.Inf(1)}
	configs[2].Gearings = make([]float64, 20)
	huge := uint32(math.MaxUint32)
	configs[3].FixingDays = &huge
	configs[4].IssueDate = &Date{}
	for _, cfg := range configs {
		if _, e := s.NewFloatingRateBond(cfg); e == nil {
			t.Fatal("invalid floating config accepted")
		}
	}
	other, e := s.NewSettings()
	bondCheck(t, e)
	cfg := m.config
	cfg.Settings = other
	if _, e := s.NewFloatingRateBond(cfg); e == nil {
		t.Fatal("different index settings accepted")
	}
	s2, e := NewSession()
	bondCheck(t, e)
	defer s2.Close()
	foreign, e := s2.NewBlackIborCouponPricer(nil)
	bondCheck(t, e)
	if e := m.bond.SetCouponPricer(foreign); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("cross-session pricer: %v", e)
	}
	moving, e := s.ConstantOptionletVolatility(ConstantRateVolConfig{Settings: other, Calendar: m.calendar, Convention: Following, Volatility: .2, DayCounter: m.counter, VolatilityType: ShiftedLognormal})
	bondCheck(t, e)
	wrong, e := s.NewBlackIborCouponPricer(moving)
	bondCheck(t, e)
	if e := m.bond.SetCouponPricer(wrong); e == nil {
		t.Fatal("different moving volatility settings accepted")
	}
	v, e = m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, atPar.Initial.NPV)
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Go(func() {
			for j := 0; j < 10; j++ {
				v, e := m.bond.NPV()
				if e != nil || math.IsNaN(v) || math.IsInf(v, 0) {
					t.Errorf("concurrent %g %v", v, e)
				}
			}
		})
	}
	wg.Wait()
}
func TestFloatingBondZeroGearingAndKnownFixingNeedNoForecastVolatility(t *testing.T) {
	cases := floatingOracles(t)
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, cases[4], false)
	v, e := m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, v, cases[4].Initial.NPV)
	historical := cases[14]
	s2, e := NewSession()
	bondCheck(t, e)
	defer s2.Close()
	m2 := floatingFixture(t, s2, historical, false)
	cfg := m2.config
	cap := .026
	cfg.Caps = []*float64{&cap}
	b, e := s2.NewFloatingRateBond(cfg)
	bondCheck(t, e)
	bondCheck(t, b.SetEngine(m2.engine))
	none, e := s2.NewBlackIborCouponPricer(nil)
	bondCheck(t, e)
	bondCheck(t, b.SetCouponPricer(none))
	bondCheck(t, m2.index.AddFixing(testDate(t, 13, 7, 2024), .0275))
	leg, e := b.Cashflows()
	bondCheck(t, e)
	flow, e := leg.At(0)
	bondCheck(t, e)
	amount, e := flow.Amount()
	bondCheck(t, e)
	bondNear(t, amount, 1000*.026*184/360)
	if _, e := b.NPV(); e == nil {
		t.Fatal("unknown future caps priced without volatility")
	}
	bondCheck(t, flow.Close())
	bondCheck(t, leg.Close())
}

func TestFloatingBondIrrelevantVolatilityAndSkippedFlows(t *testing.T) {
	cases := floatingOracles(t)
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, cases[0], true)
	moving, e := s.ConstantOptionletVolatility(ConstantRateVolConfig{Settings: m.settings, SettlementDays: 100000, Calendar: m.calendar, Convention: Following, Volatility: .2, DayCounter: m.counter, VolatilityType: ShiftedLognormal})
	bondCheck(t, e)
	bad, e := s.NewBlackIborCouponPricer(moving)
	bondCheck(t, e)
	cap := .032
	cfg := m.config
	cfg.Caps = []*float64{nil, &cap}
	b, e := s.NewFloatingRateBond(cfg)
	bondCheck(t, e)
	bondCheck(t, b.SetEngine(m.engine))
	bondCheck(t, b.SetCouponPricer(m.pricer))
	old, e := b.NPV()
	bondCheck(t, e)
	leg, e := b.Cashflows()
	bondCheck(t, e)
	defer leg.Close()
	plain, e := leg.At(0)
	bondCheck(t, e)
	defer plain.Close()
	capped, e := leg.At(1)
	bondCheck(t, e)
	defer capped.Close()
	redemption, e := leg.At(-1)
	bondCheck(t, e)
	defer redemption.Close()
	bondCheck(t, b.Freeze())
	bondCheck(t, b.SetCouponPricer(bad))
	v, e := b.NPV()
	bondCheck(t, e)
	bondNear(t, v, old)
	v, e = plain.Amount()
	bondCheck(t, e)
	bondNear(t, v, cases[0].Initial.Cashflows[0])
	v, e = redemption.Amount()
	bondCheck(t, e)
	bondNear(t, v, 1000)
	if _, e := capped.Amount(); e == nil {
		t.Fatal("invalid moving reference accepted for queried cap")
	}
	bondCheck(t, b.Unfreeze())
	if _, e := b.NPV(); e == nil {
		t.Fatal("active cap priced with invalid moving reference")
	}
	v, e = b.AccruedAmount(nil)
	bondCheck(t, e)
	bondNear(t, v, 0)
	maturity := testDate(t, 16, 7, 2026)
	other, e := s.NewSettings()
	bondCheck(t, e)
	v, e = leg.NPV(CashFlowsNPVConfig{Discount: m.discountCurve, Settings: other, SettlementDate: &maturity})
	bondCheck(t, e)
	bondNear(t, v, 0)
	bondCheck(t, m.settings.SetEvaluationDate(maturity))
	v, e = b.NPV()
	bondCheck(t, e)
	bondNear(t, v, 0)
	bondCheck(t, m.settings.SetEvaluationDate(testDate(t, 15, 1, 2024)))
	allAbsent := m.config
	allAbsent.Caps = []*float64{nil}
	plainBond, e := s.NewFloatingRateBond(allAbsent)
	bondCheck(t, e)
	bondCheck(t, plainBond.SetEngine(m.engine))
	bondCheck(t, plainBond.SetCouponPricer(bad))
	v, e = plainBond.NPV()
	bondCheck(t, e)
	bondNear(t, v, cases[0].Initial.NPV)
	exCfg := m.config
	exCfg.Caps = []*float64{&cap, nil}
	exCfg.ExCouponPeriod = &Period{190, Days}
	exCfg.ExCouponCalendar = m.calendar
	exBond, e := s.NewFloatingRateBond(exCfg)
	bondCheck(t, e)
	bondCheck(t, exBond.SetCouponPricer(bad))
	bondCheck(t, m.settings.SetEvaluationDate(testDate(t, 11, 7, 2024)))
	freshCurve, e := s.NewFlatForward(testDate(t, 11, 7, 2024), .04, m.counter)
	bondCheck(t, e)
	freshEngine, e := s.NewDiscountingBondEngine(freshCurve, m.settings, nil)
	bondCheck(t, e)
	bondCheck(t, exBond.SetEngine(freshEngine))
	if _, e := exBond.NPV(); e != nil {
		t.Fatalf("excluded forecast cap was guarded: %v", e)
	}
	exLeg, e := exBond.Cashflows()
	bondCheck(t, e)
	defer exLeg.Close()
	if _, e := exLeg.NPV(CashFlowsNPVConfig{Discount: freshCurve, Settings: m.settings}); e != nil {
		t.Fatalf("excluded leg optionlet was guarded: %v", e)
	}
}

func TestFloatingBondFrozenQuotesGuardOnlyCurrentAccrual(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, floatingOracles(t)[0], true)
	start := testDate(t, 18, 1, 2024)
	schedule, e := s.NewSchedule(ScheduleConfig{Start: start, End: testDate(t, 18, 1, 2025), Frequency: Semiannual, Calendar: m.calendar, Convention: Unadjusted})
	bondCheck(t, e)
	cfg := m.config
	cfg.Schedule = schedule
	cfg.IssueDate = &start
	cfg.SettlementDays = 5
	cap := .04
	cfg.Caps = []*float64{&cap}
	b, e := s.NewFloatingRateBond(cfg)
	bondCheck(t, e)
	bondCheck(t, b.SetEngine(m.engine))
	bondCheck(t, b.SetCouponPricer(m.pricer))
	npv, e := b.NPV()
	bondCheck(t, e)
	value, e := b.SettlementValue()
	bondCheck(t, e)
	dirty, e := b.DirtyPrice()
	bondCheck(t, e)
	moving, e := s.ConstantOptionletVolatility(ConstantRateVolConfig{Settings: m.settings, SettlementDays: 100000, Calendar: m.calendar, Convention: Following, Volatility: .2, DayCounter: m.counter, VolatilityType: ShiftedLognormal})
	bondCheck(t, e)
	bad, e := s.NewBlackIborCouponPricer(moving)
	bondCheck(t, e)
	bondCheck(t, b.Freeze())
	bondCheck(t, b.SetCouponPricer(bad))
	got, e := b.NPV()
	bondCheck(t, e)
	bondNear(t, got, npv)
	got, e = b.SettlementValue()
	bondCheck(t, e)
	bondNear(t, got, value)
	got, e = b.DirtyPrice()
	bondCheck(t, e)
	bondNear(t, got, dirty)
	if _, e := b.CleanPrice(); e == nil {
		t.Fatal("current capped accrual ignored invalid surface")
	}
	if _, e := b.AccruedAmount(nil); e == nil {
		t.Fatal("default current accrual ignored invalid surface")
	}
	before := testDate(t, 15, 1, 2024)
	got, e = b.AccruedAmount(&before)
	bondCheck(t, e)
	bondNear(t, got, 0)
}

func TestFloatingBondRetainsCustomCalendarBoundsAndRecoversWithHistory(t *testing.T) {
	for _, limitedValue := range []bool{false, true} {
		t.Run(map[bool]string{false: "maturity", true: "value"}[limitedValue], func(t *testing.T) {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			settings, e := s.NewSettings()
			bondCheck(t, e)
			today := testDate(t, 15, 1, 2040)
			bondCheck(t, settings.SetEvaluationDate(today))
			cal, e := s.NullCalendar()
			bondCheck(t, e)
			limited, e := s.Uzbekistan()
			bondCheck(t, e)
			dc, e := s.Actual360()
			bondCheck(t, e)
			currency, e := s.USD()
			bondCheck(t, e)
			curve, e := s.NewFlatForward(today, .03, dc)
			bondCheck(t, e)
			valueCal, maturityCal := cal, limited
			year := 2040
			if limitedValue {
				valueCal, maturityCal = limited, cal
				year = 2041
			}
			index, e := s.NewCustomIborIndex(IborIndexConfig{FamilyName: "CalendarBoundBond", Tenor: Period{6, Months}, SettlementDays: 2, Currency: currency, FixingCalendar: cal, ValueCalendar: valueCal, MaturityCalendar: maturityCal, Convention: Following, DayCounter: dc, Forwarding: curve, Settings: settings})
			bondCheck(t, e)
			start := testDate(t, 15, 11, year)
			schedule, e := s.NewSchedule(ScheduleConfig{Start: start, End: testDate(t, 15, 5, year+1), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted})
			bondCheck(t, e)
			cap := .04
			b, e := s.NewFloatingRateBond(FloatingRateBondConfig{FaceAmount: 1000, Schedule: schedule, Index: index, AccrualDayCounter: dc, Settings: settings, Caps: []*float64{&cap}})
			bondCheck(t, e)
			pricer, e := s.NewBlackIborCouponPricer(nil)
			bondCheck(t, e)
			bondCheck(t, b.SetCouponPricer(pricer))
			leg, e := b.Cashflows()
			bondCheck(t, e)
			defer leg.Close()
			flow, e := leg.At(0)
			bondCheck(t, e)
			defer flow.Close()
			for _, close := range []func() error{cal.Close, limited.Close, schedule.Close, pricer.Close, currency.Close} {
				bondCheck(t, close())
			}
			if _, e := flow.Amount(); e == nil {
				t.Fatal("forecast crossed retained calendar horizon")
			}
			bondCheck(t, settings.SetEvaluationDate(testDate(t, 1, 12, year)))
			bondCheck(t, index.AddFixing(testDate(t, 13, 11, year), .03))
			amount, e := flow.Amount()
			bondCheck(t, e)
			days := testDate(t, 15, 5, year+1).DaysSince(start)
			bondNear(t, amount, 1000*.03*float64(days)/360)
		})
	}
}

func TestFloatingBondAllAbsentOptionsAndDefaultExCouponCalendar(t *testing.T) {
	cases := floatingOracles(t)
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, cases[9], false)
	if _, e := m.bond.NPV(); e == nil {
		t.Fatal("nonempty absent options silently installed default pricer")
	}
	none, e := s.NewBlackIborCouponPricer(nil)
	bondCheck(t, e)
	bondCheck(t, m.bond.SetCouponPricer(none))
	got, e := m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, got, cases[9].Initial.NPV)
	ex := floatingFixture(t, s, cases[15], true)
	bondCheck(t, ex.index.AddFixing(testDate(t, 13, 7, 2024), .0275))
	cfg := ex.config
	cfg.ExCouponCalendar = nil
	b, e := s.NewFloatingRateBond(cfg)
	bondCheck(t, e)
	bondCheck(t, b.SetEngine(ex.engine))
	bondCheck(t, b.SetCouponPricer(ex.pricer))
	floatingSnapshotCheck(t, b, cases[15].Initial)
}

func TestFloatingBondUnadjustedArithmeticBeyondHolidayTable(t *testing.T) {
	data, e := os.ReadFile("testdata/floating_bonds_oracle.json")
	bondCheck(t, e)
	var oracle struct {
		Arithmetic struct{ Amount, NPV, Dirty float64 } `json:"calendar_arithmetic"`
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	today := testDate(t, 24, 12, 2014)
	bondCheck(t, settings.SetEvaluationDate(today))
	dc, e := s.Actual360()
	bondCheck(t, e)
	fixingCalendar, e := s.Japan()
	bondCheck(t, e)
	valueCalendar, e := s.NullCalendar()
	bondCheck(t, e)
	maturityCalendar, e := s.Indonesia()
	bondCheck(t, e)
	currency, e := s.EUR()
	bondCheck(t, e)
	forward, e := s.NewFlatForward(today, .043, dc)
	bondCheck(t, e)
	discount, e := s.NewFlatForward(today, .023, dc)
	bondCheck(t, e)
	index, e := s.NewCustomIborIndex(IborIndexConfig{FamilyName: "UnadjustedCalendarArithmetic", Tenor: Period{3, Months}, SettlementDays: 3, Currency: currency, FixingCalendar: fixingCalendar, ValueCalendar: valueCalendar, MaturityCalendar: maturityCalendar, Convention: Unadjusted, DayCounter: dc, Forwarding: forward, Settings: settings})
	bondCheck(t, e)
	backward := Backward
	schedule, e := s.NewSchedule(ScheduleConfig{Start: testDate(t, 30, 12, 2014), End: testDate(t, 30, 3, 2015), Frequency: Quarterly, Calendar: valueCalendar, Convention: Unadjusted, Rule: &backward})
	bondCheck(t, e)
	fixingDays := uint32(2)
	payment := Unadjusted
	redemption := 101.
	b, e := s.NewFloatingRateBond(FloatingRateBondConfig{FaceAmount: 375, Schedule: schedule, Index: index, AccrualDayCounter: dc, Settings: settings, PaymentConvention: &payment, FixingDays: &fixingDays, Gearings: []float64{1.25}, Spreads: []float64{.002}, Redemption: &redemption})
	bondCheck(t, e)
	engine, e := s.NewDiscountingBondEngine(discount, settings, nil)
	bondCheck(t, e)
	bondCheck(t, b.SetEngine(engine))
	leg, e := b.Cashflows()
	bondCheck(t, e)
	defer leg.Close()
	flow, e := leg.At(0)
	bondCheck(t, e)
	defer flow.Close()
	amount, e := flow.Amount()
	bondCheck(t, e)
	bondNear(t, amount, oracle.Arithmetic.Amount)
	npv, e := b.NPV()
	bondCheck(t, e)
	bondNear(t, npv, oracle.Arithmetic.NPV)
	dirty, e := b.DirtyPrice()
	bondCheck(t, e)
	bondNear(t, dirty, oracle.Arithmetic.Dirty)
}
