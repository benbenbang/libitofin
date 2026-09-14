package itofin

import (
	"math"
	"sync"
	"testing"
)

func ratesMust[T any](t *testing.T, v T, e error) T {
	t.Helper()
	if e != nil {
		t.Fatal(e)
	}
	return v
}
func ratesOK(t *testing.T, e error) {
	t.Helper()
	if e != nil {
		t.Fatal(e)
	}
}
func TestRatesParSwapsAndFRA(t *testing.T) {
	s, e := NewSession()
	ratesOK(t, e)
	defer s.Close()
	today, e := NewDate(7, 7, 2026)
	ratesOK(t, e)
	settings, e := s.NewSettings()
	ratesOK(t, e)
	ratesOK(t, settings.SetEvaluationDate(today))
	dc, e := s.Actual360()
	ratesOK(t, e)
	curve, e := s.NewFlatForward(today, .04, dc)
	ratesOK(t, e)
	index, e := s.NewEuriborSixMonths(curve, settings)
	ratesOK(t, e)
	effective, e := NewDate(9, 7, 2026)
	ratesOK(t, e)
	swap, e := s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{5, Years}, Index: index, Settings: settings, EffectiveDate: &effective})
	ratesOK(t, e)
	value, e := swap.NPV()
	ratesOK(t, e)
	if math.Abs(value) > 1e-8 {
		t.Fatalf("par swap NPV %g", value)
	}
	fair, e := swap.FairRate()
	ratesOK(t, e)
	fixed, e := swap.FixedRate()
	ratesOK(t, e)
	if math.Abs(fair-fixed) > 1e-12 {
		t.Fatal("par rate mismatch")
	}
	nominal, e := swap.Nominal()
	ratesOK(t, e)
	if nominal != 1 {
		t.Fatal(nominal)
	}
	ratesOK(t, swap.Calculate())
	calculated, e := swap.IsCalculated()
	ratesOK(t, e)
	if !calculated {
		t.Fatal("uncached swap")
	}
	snapshot, e := swap.Results()
	ratesOK(t, e)
	if snapshot.NPV == nil || math.Abs(*snapshot.NPV) > 1e-8 {
		t.Fatal("invalid snapshot")
	}
	overnight, e := s.NewEstr(curve, settings)
	ratesOK(t, e)
	ois, e := s.MakeOis(MakeOisConfig{Tenor: Period{2, Years}, Index: overnight, Settings: settings, EffectiveDate: &effective})
	ratesOK(t, e)
	value, e = ois.Price()
	ratesOK(t, e)
	if math.Abs(value) > 1e-8 {
		t.Fatalf("par OIS NPV %g", value)
	}
	ratesOK(t, ois.Calculate())
	oisCached, e := ois.IsCalculated()
	ratesOK(t, e)
	if !oisCached {
		t.Fatal("uncached OIS")
	}
	oisResults, e := ois.Results()
	ratesOK(t, e)
	if oisResults.NPV == nil || math.Abs(*oisResults.NPV) > 1e-8 {
		t.Fatal("bad OIS snapshot")
	}
	oisFair, e := ois.FairRate()
	ratesOK(t, e)
	oisFixed, e := ois.FixedRate()
	ratesOK(t, e)
	if math.Abs(oisFair-oisFixed) > 1e-12 {
		t.Fatal("OIS fair rate")
	}
	oisNominal, e := ois.Nominal()
	ratesOK(t, e)
	if oisNominal != 1 {
		t.Fatal(oisNominal)
	}
	end, e := NewDate(9, 10, 2026)
	ratesOK(t, e)
	fra, e := s.NewForwardRateAgreement(FRAConfig{Index: index, ValueDate: effective, MaturityDate: &end, Position: PositionLong, Strike: .02, Notional: 100})
	ratesOK(t, e)
	zero := Date{}
	if _, err := s.NewForwardRateAgreement(FRAConfig{Index: index, ValueDate: effective, MaturityDate: &zero, Notional: 100}); err == nil {
		t.Fatal("explicit zero maturity accepted")
	}
	term, e := dc.YearFraction(effective, end)
	ratesOK(t, e)
	forward := math.Expm1(.04*term) / term
	got, e := fra.ForwardRate()
	ratesOK(t, e)
	if math.Abs(got-forward) > 1e-12 {
		t.Fatal(got, forward)
	}
	amount, e := fra.Amount()
	ratesOK(t, e)
	expected := 100 * (forward - .02) * term / (1 + forward*term)
	if math.Abs(amount-expected) > 1e-12 {
		t.Fatal(amount, expected)
	}
	days, e := dc.YearFraction(today, effective)
	ratesOK(t, e)
	npv, e := fra.NPV()
	ratesOK(t, e)
	if math.Abs(npv-expected*math.Exp(-.04*days)) > 1e-12 {
		t.Fatal(npv)
	}
	d, e := fra.ValueDate()
	ratesOK(t, e)
	if d != effective {
		t.Fatal(d)
	}
	d, e = fra.MaturityDate()
	ratesOK(t, e)
	if d != end {
		t.Fatal(d)
	}
	// Every dependency can close while instruments retain the native graph.
	ratesOK(t, index.Close())
	ratesOK(t, curve.Close())
	ratesOK(t, overnight.Close())
	var wg sync.WaitGroup
	for n := 0; n < 8; n++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			v, e := swap.NPV()
			if e != nil || math.Abs(v) > 1e-8 {
				t.Errorf("concurrent valuation %g: %v", v, e)
			}
		}()
	}
	wg.Wait()
	ratesOK(t, swap.Close())
	if _, e = swap.NPV(); e == nil {
		t.Fatal("closed swap accepted")
	}
}
func TestRatesSwaptionCachedValue(t *testing.T) {
	s, e := NewSession()
	ratesOK(t, e)
	defer s.Close()
	today, e := NewDate(13, 3, 2002)
	ratesOK(t, e)
	settings, e := s.NewSettings()
	ratesOK(t, e)
	ratesOK(t, settings.SetEvaluationDate(today))
	calendar, e := s.Target()
	ratesOK(t, e)
	dc, e := s.Actual365Fixed()
	ratesOK(t, e)
	fixedDC, e := s.Thirty360BondBasis()
	ratesOK(t, e)
	settlement, e := calendar.Advance(today, 2, Days, Following, false)
	ratesOK(t, e)
	curve, e := s.NewFlatForward(settlement, .05, dc)
	ratesOK(t, e)
	index, e := s.NewEuriborSixMonths(curve, settings)
	ratesOK(t, e)
	exerciseDate, e := calendar.Advance(settlement, 5, Years, Following, false)
	ratesOK(t, e)
	start, e := calendar.Advance(exerciseDate, 2, Days, Following, false)
	ratesOK(t, e)
	rate := .06
	tenor := Period{1, Years}
	swap, e := s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{10, Years}, Index: index, Settings: settings, FixedRate: &rate, EffectiveDate: &start, FixedLegTenor: &tenor, FixedLegDayCounter: fixedDC})
	ratesOK(t, e)
	exercise, e := s.NewEuropeanExercise(exerciseDate)
	ratesOK(t, e)
	option, e := s.NewSwaption(SwaptionConfig{Swap: swap, Exercise: exercise, Settings: settings})
	ratesOK(t, e)
	if _, e = option.NPV(); e == nil {
		t.Fatal("missing engine accepted")
	}
	quote, e := s.NewSimpleQuote(.20)
	ratesOK(t, e)
	engine, e := s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: quote, DayCounter: dc, Settings: settings})
	ratesOK(t, e)
	value, e := option.Price(engine)
	ratesOK(t, e)
	if math.Abs(value-.036418158579) > 1e-12 {
		t.Fatalf("QuantLib cached oracle: %.15g", value)
	}
	snapshot, e := option.Results()
	ratesOK(t, e)
	ratesOK(t, quote.SetValue(.30))
	updated, e := option.NPV()
	ratesOK(t, e)
	if updated <= value || snapshot.NPV == nil || *snapshot.NPV != value {
		t.Fatal("quote observation or snapshot isolation failed")
	}
	other, e := NewSession()
	ratesOK(t, e)
	defer other.Close()
	foreign, e := other.NewSettings()
	ratesOK(t, e)
	if _, e = s.NewSwaption(SwaptionConfig{Swap: swap, Exercise: exercise, Settings: foreign}); e == nil {
		t.Fatal("foreign settings accepted")
	}
	ratesOK(t, engine.Close())
	ratesOK(t, swap.Close())
	ratesOK(t, exercise.Close())
	_, e = option.NPV()
	ratesOK(t, e)
}

func TestRatesCapFloorLegsAndSwapIndex(t *testing.T) {
	s, e := NewSession()
	ratesOK(t, e)
	defer s.Close()
	today, e := NewDate(7, 7, 2026)
	ratesOK(t, e)
	start, e := NewDate(9, 7, 2027)
	ratesOK(t, e)
	end, e := NewDate(9, 7, 2029)
	ratesOK(t, e)
	settings, e := s.NewSettings()
	ratesOK(t, e)
	ratesOK(t, settings.SetEvaluationDate(today))
	dc, e := s.Actual360()
	ratesOK(t, e)
	cal, e := s.Target()
	ratesOK(t, e)
	currency, e := s.EUR()
	ratesOK(t, e)
	curve, e := s.NewFlatForward(today, .04, dc)
	ratesOK(t, e)
	index, e := s.NewEuriborSixMonths(curve, settings)
	ratesOK(t, e)
	schedule, e := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: ModifiedFollowing})
	ratesOK(t, e)
	base, e := s.NewIborLeg(schedule, index)
	ratesOK(t, e)
	leg, e := base.WithNotional(100)
	ratesOK(t, e)
	leg, e = leg.WithPaymentDayCounter(dc)
	ratesOK(t, e)
	leg, e = leg.WithPaymentAdjustment(ModifiedFollowing)
	ratesOK(t, e)
	leg, e = leg.WithFixingDays(2)
	ratesOK(t, e)
	count, e := leg.CouponCount()
	ratesOK(t, e)
	if count != 4 {
		t.Fatal(count)
	}
	flows, e := leg.Build()
	ratesOK(t, e)
	size, e := flows.Len()
	ratesOK(t, e)
	if size != count {
		t.Fatal(size)
	}
	flow, e := flows.At(-1)
	ratesOK(t, e)
	amount, e := flow.Amount()
	ratesOK(t, e)
	if amount <= 0 {
		t.Fatal(amount)
	}
	_, e = flow.Date()
	ratesOK(t, e)
	if _, e = flows.At(-count - 1); e == nil {
		t.Fatal("invalid negative index accepted")
	}
	value, e := flows.NPV(CashFlowsNPVConfig{Discount: curve, Settings: settings})
	ratesOK(t, e)
	if value <= 0 {
		t.Fatal(value)
	}
	cap, e := s.NewCap(leg, []float64{.04}, settings)
	ratesOK(t, e)
	floor, e := s.NewFloor(leg, []float64{.04}, settings)
	ratesOK(t, e)
	collar, e := s.NewCollar(leg, []float64{.04}, []float64{.04}, settings)
	ratesOK(t, e)
	quote, e := s.NewSimpleQuote(.20)
	ratesOK(t, e)
	engine, e := s.NewBlackCapFloorEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: quote, DayCounter: dc, Settings: settings})
	ratesOK(t, e)
	capValue, e := cap.Price(engine)
	ratesOK(t, e)
	floorValue, e := floor.Price(engine)
	ratesOK(t, e)
	collarValue, e := collar.Price(engine)
	ratesOK(t, e)
	if capValue <= 0 || floorValue <= 0 || math.Abs(collarValue-(capValue-floorValue)) > 1e-12 {
		t.Fatal("cap-floor-collar identity", capValue, floorValue, collarValue)
	}
	caps, e := cap.CapRates()
	ratesOK(t, e)
	floors, e := floor.FloorRates()
	ratesOK(t, e)
	if len(caps) != count || len(floors) != count || caps[0] != .04 || floors[0] != .04 {
		t.Fatal(caps, floors)
	}
	n, e := cap.CouponCount()
	ratesOK(t, e)
	if n != count {
		t.Fatal(n)
	}
	ratesOK(t, cap.Calculate())
	calculated, e := cap.IsCalculated()
	ratesOK(t, e)
	if !calculated {
		t.Fatal("uncached cap")
	}
	_, e = cap.Results()
	ratesOK(t, e)
	shift, e := engine.Displacement()
	ratesOK(t, e)
	if shift != 0 {
		t.Fatal(shift)
	}
	swapIndex, e := s.NewSwapIndex(SwapIndexConfig{Family: "TestSwap", Tenor: Period{5, Years}, SettlementDays: 2, Currency: currency, Calendar: cal, FixedLegTenor: Period{1, Years}, FixedLegConvention: ModifiedFollowing, FixedLegDayCounter: dc, Index: index, Discount: curve, Settings: settings})
	ratesOK(t, e)
	fixing, e := swapIndex.Fixing(start, false)
	ratesOK(t, e)
	if fixing <= 0 {
		t.Fatal(fixing)
	}
	exogenous, e := swapIndex.ExogenousDiscount()
	ratesOK(t, e)
	if !exogenous {
		t.Fatal("missing exogenous discount")
	}
	tenor, e := swapIndex.FixedLegTenor()
	ratesOK(t, e)
	if tenor != (Period{1, Years}) {
		t.Fatal(tenor)
	}
	gotCurrency, e := swapIndex.Currency()
	ratesOK(t, e)
	code, e := gotCurrency.Code()
	ratesOK(t, e)
	if code != "EUR" {
		t.Fatal(code)
	}
	ratesOK(t, leg.Close())
	ratesOK(t, flows.Close())
	_, e = flow.Amount()
	ratesOK(t, e)
	_, e = cap.NPV()
	ratesOK(t, e)
}

func TestRatesEngineFactoriesAndAtomicPrice(t *testing.T) {
	s, e := NewSession()
	ratesOK(t, e)
	defer s.Close()
	today, e := NewDate(7, 7, 2026)
	ratesOK(t, e)
	settings, e := s.NewSettings()
	ratesOK(t, e)
	ratesOK(t, settings.SetEvaluationDate(today))
	cal, e := s.Target()
	ratesOK(t, e)
	dc, e := s.Actual365Fixed()
	ratesOK(t, e)
	floatDC, e := s.Actual360()
	ratesOK(t, e)
	curve, e := s.NewFlatForward(today, .04, dc)
	ratesOK(t, e)
	index, e := s.NewEuriborSixMonths(curve, settings)
	ratesOK(t, e)
	exerciseDate, e := cal.Advance(today, 1, Years, Following, false)
	ratesOK(t, e)
	start, e := cal.Advance(exerciseDate, 2, Days, Following, false)
	ratesOK(t, e)
	end, e := cal.Advance(start, 3, Years, Following, false)
	ratesOK(t, e)
	fixed, e := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Annual, Calendar: cal, Convention: ModifiedFollowing})
	ratesOK(t, e)
	floating, e := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: ModifiedFollowing})
	ratesOK(t, e)
	swap, e := s.NewVanillaSwap(VanillaSwapConfig{Type: SwapPayer, Nominal: 1, FixedRate: .04, FixedSchedule: fixed, FloatingSchedule: floating, FixedDayCounter: dc, FloatingDayCounter: floatDC, Index: index, Settings: settings})
	ratesOK(t, e)
	_, e = swap.Price(curve, settings)
	ratesOK(t, e)
	exercise, e := s.NewEuropeanExercise(exerciseDate)
	ratesOK(t, e)
	option, e := s.NewSwaption(SwaptionConfig{Swap: swap, Exercise: exercise, Settings: settings})
	ratesOK(t, e)
	blackQuote, e := s.NewSimpleQuote(.20)
	ratesOK(t, e)
	normalQuote, e := s.NewSimpleQuote(.01)
	ratesOK(t, e)
	blackFlat, e := s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: blackQuote, DayCounter: dc, Settings: settings})
	ratesOK(t, e)
	normalFlat, e := s.NewBachelierSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: normalQuote, DayCounter: dc, Settings: settings})
	ratesOK(t, e)
	cfg := ConstantRateVolConfig{Calendar: cal, Convention: Following, DayCounter: dc, Settings: settings, Quote: blackQuote, VolatilityType: ShiftedLognormal}
	blackVol, e := s.ConstantSwaptionVolatility(cfg)
	ratesOK(t, e)
	blackSurface, e := s.NewBlackSwaptionEngine(SwaptionEngineConfig{Volatility: blackVol, Discount: curve, Settings: settings})
	ratesOK(t, e)
	cfg.Quote = normalQuote
	cfg.VolatilityType = Normal
	normalVol, e := s.ConstantSwaptionVolatility(cfg)
	ratesOK(t, e)
	normalSurface, e := s.NewBachelierSwaptionEngine(SwaptionEngineConfig{Volatility: normalVol, Discount: curve, Settings: settings})
	ratesOK(t, e)
	blackValue, e := option.Price(blackFlat)
	ratesOK(t, e)
	surfaceValue, e := option.Price(blackSurface)
	ratesOK(t, e)
	if math.Abs(blackValue-surfaceValue) > 1e-12 {
		t.Fatal("Black factories differ", blackValue, surfaceValue)
	}
	ratesOK(t, option.SetBachelierEngine(normalFlat))
	normalValue, e := option.NPV()
	ratesOK(t, e)
	ratesOK(t, option.SetBachelierEngine(normalSurface))
	surfaceValue, e = option.NPV()
	ratesOK(t, e)
	if math.Abs(normalValue-surfaceValue) > 1e-12 {
		t.Fatal("Bachelier factories differ", normalValue, surfaceValue)
	}
	ratesOK(t, option.Calculate())
	cached, e := option.IsCalculated()
	ratesOK(t, e)
	if !cached {
		t.Fatal("uncached swaption")
	}
	hw, e := s.NewHullWhite(curve, .03, .01)
	ratesOK(t, e)
	ratesOK(t, option.SetJamshidianEngine(hw))
	v, e := option.NPV()
	ratesOK(t, e)
	if v <= 0 || math.IsNaN(v) {
		t.Fatal("invalid Jamshidian value", v)
	}
	// Different Black engines price the same object concurrently. Price must
	// select and use its engine within one serialized worker invocation.
	highQuote, e := s.NewSimpleQuote(.40)
	ratesOK(t, e)
	high, e := s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: highQuote, DayCounter: dc, Settings: settings})
	ratesOK(t, e)
	highValue, e := option.Price(high)
	ratesOK(t, e)
	var wg sync.WaitGroup
	for n := 0; n < 40; n++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			engine, want := blackFlat, blackValue
			if n%2 == 1 {
				engine, want = high, highValue
			}
			got, e := option.Price(engine)
			if e != nil || math.Abs(got-want) > 1e-12 {
				t.Errorf("interleaved engine: %g want %g (%v)", got, want, e)
			}
		}(n)
	}
	wg.Wait()
	cap, e := s.NewCapFloor(CapFloorConfig{Type: CapType, Tenor: Period{3, Years}, ForwardStart: Period{1, Years}, Index: index, Strike: .04, Settings: settings})
	ratesOK(t, e)
	capFlat, e := s.NewBlackCapFloorEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: blackQuote, DayCounter: dc, Settings: settings})
	ratesOK(t, e)
	cfg.Quote = blackQuote
	cfg.VolatilityType = ShiftedLognormal
	capVol, e := s.ConstantOptionletVolatility(cfg)
	ratesOK(t, e)
	capSurface, e := s.NewBlackCapFloorEngine(BlackCapFloorEngineConfig{Volatility: capVol, Discount: curve})
	ratesOK(t, e)
	flatValue, e := cap.Price(capFlat)
	ratesOK(t, e)
	surfaceValue, e = cap.Price(capSurface)
	ratesOK(t, e)
	if math.Abs(flatValue-surfaceValue) > 1e-12 {
		t.Fatal("cap/floor factories differ", flatValue, surfaceValue)
	}
	ratesOK(t, cap.SetBlackEngine(capFlat))
}
