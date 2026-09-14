package itofin

import (
	"math"
	"testing"
)

func inflProductsCheck(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}
func inflProductsNear(t *testing.T, actual, want, tol float64) {
	t.Helper()
	if math.IsNaN(actual) || math.Abs(actual-want) > tol {
		t.Fatalf("got %.16g want %.16g tolerance %g", actual, want, tol)
	}
}
func TestZeroInflationSwapPythonOracleAndSnapshot(t *testing.T) {
	s, err := NewSession()
	inflProductsCheck(t, err)
	defer s.Close()
	settings, err := s.NewSettings()
	inflProductsCheck(t, err)
	today := testDate(t, 13, 8, 2007)
	inflProductsCheck(t, settings.SetEvaluationDate(today))
	dc, err := s.Thirty360BondBasis()
	inflProductsCheck(t, err)
	cal, err := s.UnitedKingdom()
	inflProductsCheck(t, err)
	index, err := s.NewUKRPI(settings)
	inflProductsCheck(t, err)
	inflProductsCheck(t, index.AddFixing(testDate(t, 1, 5, 2007), 205))
	inflProductsCheck(t, index.AddFixing(testDate(t, 1, 7, 2007), 207.3))
	dates := []Date{testDate(t, 1, 7, 2007), testDate(t, 1, 9, 2007), testDate(t, 1, 1, 2008), testDate(t, 1, 7, 2008), testDate(t, 1, 7, 2009)}
	curve, err := s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: today, Frequency: Monthly, DayCounter: dc}, dates, []float64{.02, .022, .025, .027, .030})
	inflProductsCheck(t, err)
	swap, err := s.NewZeroCouponInflationSwap(ZeroCouponInflationSwapConfig{Type: SwapPayer, Nominal: 1e6, Start: today, Maturity: testDate(t, 13, 8, 2008), FixedCalendar: cal, FixedConvention: ModifiedFollowing, DayCounter: dc, FixedRate: .025, Index: index, ObservationLag: Period{3, Months}, Interpolation: CpiFlat, Settings: settings})
	inflProductsCheck(t, err)
	if _, err := swap.FairRate(); err == nil {
		t.Fatal("unlinked index priced")
	}
	inflProductsCheck(t, index.LinkTo(curve))
	fair, err := swap.FairRate()
	inflProductsCheck(t, err)
	inflProductsNear(t, fair, .03336195814008680, 1e-12)
	if _, err := swap.NPV(); err == nil {
		t.Fatal("swap priced without engine")
	}
	actual365, err := s.Actual365Fixed()
	inflProductsCheck(t, err)
	discount, err := s.NewFlatForward(today, .05, actual365)
	inflProductsCheck(t, err)
	engine, err := s.NewDiscountingSwapEngine(discount, settings)
	inflProductsCheck(t, err)
	value, err := swap.Price(engine)
	inflProductsCheck(t, err)
	inflProductsNear(t, value, -7953.0510956158214, 1e-7)
	checks := []struct {
		read func() (float64, error)
		want float64
	}{{swap.FixedLegNPV, 23777.478200617854}, {swap.InflationLegNPV, -31730.529296233675}, {swap.FixedLegBPS, 95.10991280246127}}
	for _, check := range checks {
		v, err := check.read()
		inflProductsCheck(t, err)
		inflProductsNear(t, v, check.want, 1e-7)
	}
	maturity, err := swap.MaturityDate()
	inflProductsCheck(t, err)
	if maturity != testDate(t, 13, 8, 2008) {
		t.Fatal(maturity)
	}
	obs, err := swap.ObsDate()
	inflProductsCheck(t, err)
	fixing, err := swap.InflationFixingDate()
	inflProductsCheck(t, err)
	if obs != testDate(t, 13, 5, 2008) || obs != fixing {
		t.Fatal(obs, fixing)
	}
	inflProductsCheck(t, swap.Calculate())
	cached, err := swap.IsCalculated()
	inflProductsCheck(t, err)
	if !cached {
		t.Fatal("not cached")
	}
	frozen, err := swap.Results()
	inflProductsCheck(t, err)
	if frozen.NPV == nil {
		t.Fatal("missing NPV")
	}
	inflProductsNear(t, *frozen.NPV, value, 1e-12)
	// Retained dependencies remain valid after releasing external handles.
	for _, close := range []func() error{index.Close, curve.Close, discount.Close, engine.Close, dc.Close, cal.Close} {
		inflProductsCheck(t, close())
	}
	current, err := swap.NPV()
	inflProductsCheck(t, err)
	inflProductsNear(t, current, value, 1e-12)
	inflProductsCheck(t, settings.SetEvaluationDate(testDate(t, 14, 8, 2008)))
	after, err := swap.NPV()
	inflProductsCheck(t, err)
	inflProductsNear(t, after, 0, 1e-12)
	inflProductsNear(t, *frozen.NPV, value, 1e-12)
	inflProductsCheck(t, swap.Close())
	if _, err := swap.NPV(); err == nil {
		t.Fatal("released swap usable")
	}
}
func inflProductsQuotedMarket(t *testing.T) (*Session, *Settings, *YoYInflationIndex, *Calendar, *DayCounter, *YieldTermStructure) {
	t.Helper()
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
	index, err := s.NewYoYInflationIndex(YoYInflationIndexConfig{FamilyName: "YY_RPI", RegionName: "UK", RegionCode: "GB", Frequency: Monthly, AvailabilityLag: Period{1, Months}, CurrencyName: "British pound sterling", CurrencyCode: "GBP", CurrencyNumericCode: 826, CurrencySymbol: "£", CurrencyFractionSymbol: "p", CurrencyFractionsPerUnit: 100, Settings: settings})
	inflProductsCheck(t, err)
	curve, err := s.NewInterpolatedYoYInflationCurve(InflationCurveConfig{ReferenceDate: today, Frequency: Monthly, DayCounter: dc}, []Date{testDate(t, 1, 7, 2007), testDate(t, 1, 1, 2040)}, []float64{.03, .03})
	inflProductsCheck(t, err)
	inflProductsCheck(t, index.LinkTo(curve))
	discountDC, err := s.Actual360()
	inflProductsCheck(t, err)
	discount, err := s.NewFlatForward(today, .05, discountDC)
	inflProductsCheck(t, err)
	return s, settings, index, cal, dc, discount
}
func TestYoYInflationSwapFlatCurveOracle(t *testing.T) {
	s, settings, index, cal, dc, discount := inflProductsQuotedMarket(t)
	rule := Backward
	schedule, err := s.NewSchedule(ScheduleConfig{Start: testDate(t, 13, 8, 2007), End: testDate(t, 13, 8, 2012), Frequency: Annual, Calendar: cal, Convention: Unadjusted, Rule: &rule})
	inflProductsCheck(t, err)
	cfg := YearOnYearInflationSwapConfig{Type: SwapPayer, Nominal: 1e6, FixedSchedule: schedule, FixedRate: .03, FixedDayCounter: dc, YoYSchedule: schedule, Index: index, ObservationLag: Period{2, Months}, Interpolation: CpiFlat, YoYDayCounter: dc, PaymentCalendar: cal, PaymentConvention: ModifiedFollowing, Settings: settings}
	swap, err := s.NewYearOnYearInflationSwap(cfg)
	inflProductsCheck(t, err)
	engine, err := s.NewDiscountingSwapEngine(discount, settings)
	inflProductsCheck(t, err)
	value, err := swap.Price(engine)
	inflProductsCheck(t, err)
	inflProductsNear(t, value, 0, 1e-6)
	fair, err := swap.FairRate()
	inflProductsCheck(t, err)
	inflProductsNear(t, fair, .03, 1e-12)
	fixed, err := swap.FixedLegNPV()
	inflProductsCheck(t, err)
	yoy, err := swap.YoYLegNPV()
	inflProductsCheck(t, err)
	inflProductsNear(t, fixed+yoy, value, 1e-6)
	cfg.FixedRate = .035
	off, err := s.NewYearOnYearInflationSwap(cfg)
	inflProductsCheck(t, err)
	value, err = off.Price(engine)
	inflProductsCheck(t, err)
	if math.Abs(value) < 1 {
		t.Fatal("off market swap priced at zero")
	}
	fair, err = off.FairRate()
	inflProductsCheck(t, err)
	inflProductsNear(t, fair, .03, 1e-12)
	spread, err := off.FairSpread()
	inflProductsCheck(t, err)
	inflProductsNear(t, spread, .005, 1e-12)
	strike, err := off.FixedRate()
	inflProductsCheck(t, err)
	inflProductsNear(t, strike, .035, 1e-12)
	spread, err = off.Spread()
	inflProductsCheck(t, err)
	inflProductsNear(t, spread, 0, 1e-12)
	inflProductsCheck(t, off.Calculate())
	cached, err := off.IsCalculated()
	inflProductsCheck(t, err)
	if !cached {
		t.Fatal("not cached")
	}
	result, err := off.Results()
	inflProductsCheck(t, err)
	inflProductsNear(t, *result.NPV, value, 1e-12)
	other, err := NewSession()
	inflProductsCheck(t, err)
	defer other.Close()
	cfg.Settings, err = other.NewSettings()
	inflProductsCheck(t, err)
	if _, err = s.NewYearOnYearInflationSwap(cfg); err != ErrSessionMismatch {
		t.Fatal("foreign settings accepted", err)
	}
}
