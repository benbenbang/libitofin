package itofin

import (
	"math"
	"testing"
)

// QuantLib test-suite inflationcapfloorengines cached cap fixture, as exposed
// by the committed Python test_yoy_optionlet_vol.py (same 0.02 absolute tolerance).
func inflProductsCapMarket(t *testing.T) (*Session, *Settings, *YoYInflationIndex, *Calendar, *DayCounter, *YieldTermStructure) {
	s, err := NewSession()
	inflProductsCheck(t, err)
	t.Cleanup(func() { s.Close() })
	settings, err := s.NewSettings()
	inflProductsCheck(t, err)
	today := testDate(t, 13, 8, 2007)
	inflProductsCheck(t, settings.SetEvaluationDate(today))
	cal, err := s.UnitedKingdom()
	inflProductsCheck(t, err)
	dc, err := s.Thirty360BondBasis()
	inflProductsCheck(t, err)
	rpi, err := s.NewUKRPI(settings)
	inflProductsCheck(t, err)
	rule := Backward
	schedule, err := s.NewSchedule(ScheduleConfig{Start: testDate(t, 1, 1, 2005), End: today, Frequency: Monthly, Calendar: cal, Convention: ModifiedFollowing, Rule: &rule})
	inflProductsCheck(t, err)
	dates, err := schedule.Dates()
	inflProductsCheck(t, err)
	figures := []float64{189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1, 193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2, 207.3, -999., -999.}
	if len(dates) != len(figures) {
		t.Fatal("fixture schedule mismatch", len(dates))
	}
	for i, d := range dates {
		inflProductsCheck(t, rpi.AddFixing(d, figures[i]))
	}
	index, err := s.NewYoYInflationIndexFromUnderlying(rpi)
	inflProductsCheck(t, err)
	discountDC, err := s.ActualActualISDA()
	inflProductsCheck(t, err)
	discount, err := s.NewFlatForward(today, .05, discountDC)
	inflProductsCheck(t, err)
	quoteData := [][4]float64{{13, 8, 2008, 2.95}, {13, 8, 2009, 2.95}, {13, 8, 2010, 2.93}, {15, 8, 2011, 2.955}, {13, 8, 2012, 2.945}, {13, 8, 2013, 2.985}, {13, 8, 2014, 3.01}, {13, 8, 2015, 3.035}, {13, 8, 2016, 3.055}, {13, 8, 2017, 3.075}, {13, 8, 2019, 3.105}, {15, 8, 2022, 3.135}, {13, 8, 2027, 3.155}, {13, 8, 2032, 3.145}, {13, 8, 2037, 3.145}}
	helpers := make([]*YoYInflationHelper, 0, len(quoteData))
	for _, v := range quoteData {
		q, err := s.NewSimpleQuote(v[3] / 100)
		inflProductsCheck(t, err)
		helper, err := s.NewYearOnYearInflationSwapHelper(InflationHelperConfig{Quote: q, SwapObservationLag: Period{2, Months}, Maturity: testDate(t, int(v[0]), int(v[1]), int(v[2])), Calendar: cal, PaymentConvention: ModifiedFollowing, DayCounter: dc, ObservationInterpolation: CpiFlat, Settings: settings}, index, discount)
		inflProductsCheck(t, err)
		helpers = append(helpers, helper)
	}
	base, err := rpi.LastFixingDate()
	inflProductsCheck(t, err)
	curve, err := s.NewPiecewiseYoYInflationCurve(InflationCurveConfig{ReferenceDate: today, BaseDate: base, BaseYoYRate: .0295, Frequency: Monthly, DayCounter: dc}, helpers)
	inflProductsCheck(t, err)
	inflProductsCheck(t, index.LinkTo(curve))
	return s, settings, index, cal, dc, discount
}
func TestYoYCapFloorCachedPriceLiveQuoteAndOptionalBuilder(t *testing.T) {
	s, settings, index, cal, dc, discount := inflProductsCapMarket(t)
	quote, err := s.NewSimpleQuote(.01)
	inflProductsCheck(t, err)
	volcfg := ConstantYoYOptionletVolatilityConfig{Quote: quote, Calendar: cal, Convention: ModifiedFollowing, DayCounter: dc, ObservationLag: Period{0, Days}, Frequency: Annual, MinStrike: -1, MaxStrike: 100, Settings: settings}
	vol, err := s.NewConstantYoYOptionletVolatility(volcfg)
	inflProductsCheck(t, err)
	nominal, strike := 1e6, .0295
	cfg := MakeYoYInflationCapFloorConfig{Type: CapType, Index: index, Length: 2, Calendar: cal, ObservationLag: Period{0, Days}, Interpolation: CpiFlat, Settings: settings, Nominal: &nominal, Strike: &strike}
	maker, err := s.NewMakeYoYInflationCapFloor(cfg)
	inflProductsCheck(t, err)
	cap, err := maker.Build()
	inflProductsCheck(t, err)
	engine, err := s.NewBlackYoYInflationCapFloorEngine(index, vol, discount)
	inflProductsCheck(t, err)
	price, err := cap.Price(engine)
	inflProductsCheck(t, err)
	inflProductsNear(t, price, 219.452, .02)
	frozen, err := cap.Results()
	inflProductsCheck(t, err)
	inflProductsNear(t, *frozen.NPV, price, 1e-12)
	inflProductsCheck(t, quote.SetValue(.02))
	newPrice, err := cap.NPV()
	inflProductsCheck(t, err)
	if newPrice <= price {
		t.Fatal("live volatility did not reprice", price, newPrice)
	}
	inflProductsNear(t, *frozen.NPV, price, 1e-12)
	cached, err := cap.IsCalculated()
	inflProductsCheck(t, err)
	if !cached {
		t.Fatal("not cached")
	}
	inflProductsCheck(t, cap.Calculate())
	count, err := cap.CouponCount()
	inflProductsCheck(t, err)
	if count != 2 {
		t.Fatal(count)
	}
	caps, err := cap.CapRates()
	inflProductsCheck(t, err)
	floors, err := cap.FloorRates()
	inflProductsCheck(t, err)
	if len(caps) != 2 || caps[0] != strike || len(floors) != 0 {
		t.Fatal(caps, floors)
	}
	start, err := cap.StartDate()
	inflProductsCheck(t, err)
	end, err := cap.MaturityDate()
	inflProductsCheck(t, err)
	if end.Serial() <= start.Serial() {
		t.Fatal(start, end)
	}
	atm, err := cap.ATMRate(discount)
	inflProductsCheck(t, err)
	if atm <= 0 || atm > .1 {
		t.Fatal(atm)
	}
	lag, err := vol.ObservationLag()
	inflProductsCheck(t, err)
	if !lag.Equal(Period{0, Days}) {
		t.Fatal(lag)
	}
	frequency, err := vol.Frequency()
	inflProductsCheck(t, err)
	if frequency != Annual {
		t.Fatal(frequency)
	}
	interp, err := vol.IndexIsInterpolated()
	inflProductsCheck(t, err)
	if interp {
		t.Fatal("interpolated")
	}
	_, err = vol.BaseDate()
	inflProductsCheck(t, err)
	v, err := vol.Volatility(end, strike, lag)
	inflProductsCheck(t, err)
	inflProductsNear(t, v, .02, 1e-14)
	variance, err := vol.TotalVariance(end, strike, lag)
	inflProductsCheck(t, err)
	if variance <= 0 {
		t.Fatal(variance)
	}
	if _, err := vol.Volatility(end, 101, lag); err == nil {
		t.Fatal("out of range strike accepted")
	}
	for distribution, factory := range []func(*YoYInflationIndex, *ConstantYoYOptionletVolatility, *YieldTermStructure) (*YoYInflationCapFloorEngine, error){s.NewBlackYoYInflationCapFloorEngine, s.NewUnitDisplacedYoYInflationCapFloorEngine, s.NewBachelierYoYInflationCapFloorEngine} {
		eng, err := factory(index, vol, discount)
		inflProductsCheck(t, err)
		name, err := eng.Distribution()
		inflProductsCheck(t, err)
		if name != []string{"black", "unit_displaced", "bachelier"}[distribution] {
			t.Fatal(name)
		}
		p, err := cap.Price(eng)
		inflProductsCheck(t, err)
		if p <= 0 || math.IsNaN(p) {
			t.Fatal(p)
		}
	}
	// Constructor keeps deferred build semantics, while oversized lengths never truncate.
	cfg.Length = (1 << 32) + 2
	if _, err = s.NewMakeYoYInflationCapFloor(cfg); err == nil {
		t.Fatal("wrapped length accepted")
	}
	cfg.Length = 2
	cfg.Strike = nil
	bad, err := s.NewMakeYoYInflationCapFloor(cfg)
	inflProductsCheck(t, err)
	if _, err = bad.Build(); err == nil {
		t.Fatal("missing strike accepted")
	}
	cfg.ATMStrike = discount
	cfg.Engine = engine
	cfg.AsOptionlet = true
	cfg.FirstCapletExcluded = true
	cfg.ForwardStart = &Period{0, Days}
	today := testDate(t, 13, 8, 2007)
	cfg.EffectiveDate = &today
	cfg.PaymentDayCounter = dc
	conv := ModifiedFollowing
	cfg.PaymentAdjustment = &conv
	fixing := uint32(0)
	cfg.FixingDays = &fixing
	atmMaker, err := s.NewMakeYoYInflationCapFloor(cfg)
	inflProductsCheck(t, err)
	atmCap, err := atmMaker.Build()
	inflProductsCheck(t, err)
	if _, err = atmCap.NPV(); err != nil {
		t.Fatal(err)
	}
	inflProductsCheck(t, quote.Close())
	inflProductsCheck(t, vol.Close())
	if _, err = cap.Price(engine); err != nil {
		t.Fatal("dependencies not retained", err)
	}
}
