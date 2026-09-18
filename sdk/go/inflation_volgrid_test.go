package itofin

import (
	"math"
	"testing"
)

// Public QuantLib inflationvolatility.cpp EU fixture, also used by the
// committed Python test_yoy_price_surface/test_kinterpolated_yoy_vol oracles.
var inflGridTimes = []float64{0.0109589, 0.0684932, 0.263014, 0.317808, 0.567123, 0.816438, 1.06575, 1.31507, 1.56438, 2.0137, 3.01918, 4.01644, 5.01644, 6.01644, 7.01644, 8.01644, 9.02192, 10.0192, 12.0192, 15.0247, 20.0301, 25.0356, 30.0329, 40.0384, 50.0466}
var inflGridNominalRates = []float64{.0415600, .0426840, .0470980, .0458506, .0449550, .0439784, .0431887, .0426604, .0422925, .0424591, .0421477, .0421853, .0424016, .0426969, .0430804, .0435011, .0439368, .0443825, .0452589, .0463389, .0472636, .0473401, .0470629, .0461092, .0450794}
var inflGridCapStrikes = []float64{.02, .025, .03, .035, .04, .05}
var inflGridFloorStrikes = []float64{-.01, 0, .005, .01, .015, .02}
var inflGridYears = []int{3, 5, 7, 10, 15, 20, 30}
var inflGridCapPrices = [][]float64{
	{116.225, 204.945, 296.285, 434.29, 654.47, 844.775, 1132.33},
	{34.305, 71.575, 114.1, 184.33, 307.595, 421.395, 602.35},
	{6.37, 19.085, 35.635, 66.42, 127.69, 189.685, 296.195},
	{1.325, 5.745, 12.585, 26.945, 58.95, 94.08, 158.985},
	{.501, 2.37, 5.38, 13.065, 31.91, 53.95, 96.97},
	{.501, .695, 1.47, 4.415, 12.86, 23.75, 46.7},
}
var inflGridFloorPrices = [][]float64{
	{.501, .851, 2.44, 6.645, 16.23, 26.85, 46.365},
	{.501, 2.236, 5.555, 13.075, 28.46, 44.525, 73.08},
	{1.025, 3.935, 9.095, 19.64, 39.93, 60.375, 96.02},
	{2.465, 7.885, 16.155, 31.6, 59.34, 86.21, 132.045},
	{6.9, 17.92, 32.085, 56.08, 95.95, 132.85, 194.18},
	{23.52, 47.625, 74.085, 114.355, 175.72, 229.565, 316.285},
}
var inflGridYoYRates = []float64{.0237951, .0238749, .0240334, .0241934, .0243567, .0245323, .0247213, .0249348, .0251768, .0254337, .0257258, .0260217, .0263006, .0265538, .0267803, .0269378, .0270608, .0271363, .0272, .0272512, .0272927, .027317, .0273615, .0273811, .0274063, .0274307, .0274625, .027527, .0275952, .0276734, .027794}
var inflGridAllStrikes = []float64{-.01, 0, .005, .01, .015, .02, .025, .03, .035, .04, .05}

type inflGridFixture struct {
	session    *Session
	settings   *Settings
	calendar   *Calendar
	dayCounter *DayCounter
	index      *YoYInflationIndex
	nominal    *YieldTermStructure
	config     YoYCapFloorTermPriceSurfaceConfig
	prices     *YoYCapFloorTermPriceSurface
}

func inflGridDate(t *testing.T, d, m, y int) Date {
	t.Helper()
	v, e := NewDate(d, m, y)
	if e != nil {
		t.Fatal(e)
	}
	return v
}
func inflGridClose(t *testing.T, got, want, tolerance float64, err error) {
	t.Helper()
	if err != nil || math.IsNaN(got) || math.Abs(got-want) > tolerance {
		t.Fatalf("got %.15g, want %.15g +/- %g: %v", got, want, tolerance, err)
	}
}
func newInflGridFixture(t *testing.T, linkYoY bool) inflGridFixture {
	t.Helper()
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() {
		if e := s.Close(); e != nil {
			t.Error(e)
		}
	})
	eval := inflGridDate(t, 23, 11, 2007)
	settings, e := s.NewSettings()
	if e != nil {
		t.Fatal(e)
	}
	if e = settings.SetEvaluationDate(eval); e != nil {
		t.Fatal(e)
	}
	dc, e := s.Actual365Fixed()
	if e != nil {
		t.Fatal(e)
	}
	cal, e := s.Target()
	if e != nil {
		t.Fatal(e)
	}
	zero, e := s.NewEUHICP(settings)
	if e != nil {
		t.Fatal(e)
	}
	index, e := s.NewYoYInflationIndexFromUnderlying(zero)
	if e != nil {
		t.Fatal(e)
	}
	dates := make([]Date, len(inflGridTimes))
	for i, tm := range inflGridTimes {
		years := int(tm)
		days := int64((tm - float64(years)) * 365)
		dates[i], e = inflGridDate(t, 23, 11, 2007+years).AddDays(days)
		if e != nil {
			t.Fatal(e)
		}
	}
	// Truncation is required: the first time is just below 4/365.
	if dates[0] != inflGridDate(t, 26, 11, 2007) {
		t.Fatal("nominal date fixture rounded instead of truncated")
	}
	nominal, e := s.NewZeroCurve(NodeCurveConfig{Dates: dates, Values: inflGridNominalRates, DayCounter: dc, Interpolation: "Cubic"})
	if e != nil {
		t.Fatal(e)
	}
	if linkYoY {
		start, e := cal.Advance(eval, -2, Months, ModifiedFollowing, false)
		if e != nil {
			t.Fatal(e)
		}
		dates := []Date{inflGridDate(t, 1, 10, 2007)}
		for i := 1; i < len(inflGridYoYRates); i++ {
			d, e := cal.Advance(start, int32(i), Years, ModifiedFollowing, false)
			if e != nil {
				t.Fatal(e)
			}
			dates = append(dates, d)
		}
		yoy, e := s.NewInterpolatedYoYInflationCurve(InflationCurveConfig{ReferenceDate: eval, Frequency: Monthly, DayCounter: dc}, dates, inflGridYoYRates)
		if e != nil {
			t.Fatal(e)
		}
		if e = index.LinkTo(yoy); e != nil {
			t.Fatal(e)
		}
		if e = yoy.Close(); e != nil {
			t.Fatal(e)
		} // The linked index retains the curve.
	}
	var maturities []Period
	for _, n := range inflGridYears {
		maturities = append(maturities, Period{int32(n), Years})
	}
	cfg := YoYCapFloorTermPriceSurfaceConfig{ObservationLag: Period{3, Months}, Index: index, Interpolation: CpiLinear, NominalTermStructure: nominal, DayCounter: dc, Calendar: cal, Convention: ModifiedFollowing, CapStrikes: inflGridCapStrikes, FloorStrikes: inflGridFloorStrikes, Maturities: maturities, CapPrices: inflGridCapPrices, FloorPrices: inflGridFloorPrices, Settings: settings}
	prices, e := s.NewYoYCapFloorTermPriceSurface(cfg)
	if e != nil {
		t.Fatal(e)
	}
	if e = zero.Close(); e != nil {
		t.Fatal(e)
	}
	return inflGridFixture{s, settings, cal, dc, index, nominal, cfg, prices}
}
func TestYoYPriceSurfaceCachedSwapsAndGridOracle(t *testing.T) {
	f := newInflGridFixture(t, false)
	strikes, e := f.prices.Strikes()
	if e != nil || len(strikes) != len(inflGridAllStrikes) {
		t.Fatal(strikes, e)
	}
	for i, x := range strikes {
		inflGridClose(t, x, inflGridAllStrikes[i], 1e-12, nil)
	}
	maturities, e := f.prices.Maturities()
	if e != nil || len(maturities) != len(inflGridYears) {
		t.Fatal(maturities, e)
	}
	swaps := []float64{.024586, .0247575, .0249396, .0252596, .0258498, .0262883, .0267915}
	for j, n := range inflGridYears {
		if !maturities[j].Equal(Period{int32(n), Years}) {
			t.Fatal(maturities[j])
		}
		date := inflGridDate(t, 23, 11, 2007+n)
		x, e := f.prices.ATMYoYSwapRate(date, true)
		inflGridClose(t, x, swaps[j], 2e-5, e)
		// Test every node; retain the Python fixture's original 1e-9 relative tolerance.
		for i, k := range inflGridCapStrikes {
			x, e = f.prices.CapPrice(date, k)
			want := inflGridCapPrices[i][j]
			inflGridClose(t, x, want, math.Abs(want)*1e-9, e)
		}
		for i, k := range inflGridFloorStrikes {
			x, e = f.prices.FloorPrice(date, k)
			want := inflGridFloorPrices[i][j]
			inflGridClose(t, x, want, math.Abs(want)*1e-9, e)
		}
	}
	if _, e = f.prices.ATMYoYSwapRate(inflGridDate(t, 23, 11, 2008), false); e == nil {
		t.Fatal("out-of-range swap quote was not gated")
	}
	for _, o := range []object{f.index.object, f.nominal.object, f.dayCounter.object, f.calendar.object} {
		if e = o.Close(); e != nil {
			t.Fatal(e)
		}
	}
	x, e := f.prices.ATMYoYSwapRate(inflGridDate(t, 23, 11, 2010), true)
	inflGridClose(t, x, swaps[0], 2e-5, e)
}
func TestYoYPriceSurfaceRejectsInvalidShapeAndForeignSession(t *testing.T) {
	f := newInflGridFixture(t, false)
	cfg := f.config
	cfg.CapPrices = [][]float64{{1}}
	if _, e := f.session.NewYoYCapFloorTermPriceSurface(cfg); e == nil {
		t.Fatal("ragged prices accepted")
	}
	cfg = f.config
	cfg.Maturities = nil
	if _, e := f.session.NewYoYCapFloorTermPriceSurface(cfg); e == nil {
		t.Fatal("empty maturities accepted")
	}
	other, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer other.Close()
	if _, e = other.NewYoYCapFloorTermPriceSurface(f.config); e == nil {
		t.Fatal("foreign dependencies accepted")
	}
	if e = f.prices.Close(); e != nil {
		t.Fatal(e)
	}
	if _, e = f.prices.Strikes(); e == nil {
		t.Fatal("closed price surface queried")
	}
}
func TestKInterpolatedYoYVolatilityCachedSlicesOracle(t *testing.T) {
	f := newInflGridFixture(t, true)
	surface, e := f.session.NewKInterpolatedYoYOptionletVolatilitySurface(KInterpolatedYoYOptionletVolatilitySurfaceConfig{Calendar: f.calendar, Convention: ModifiedFollowing, DayCounter: f.dayCounter, ObservationLag: Period{3, Months}, CapFloorPrices: f.prices, Index: f.index, NominalTermStructure: f.nominal, Slope: -.5, Settings: f.settings})
	if e != nil {
		t.Fatal(e)
	}
	base, e := surface.BaseDate()
	if e != nil {
		t.Fatal(e)
	}
	expected := [][]float64{{.0129, .0094, .0083, .0073, .0064, .0058, .0042, .0046, .0053, .0064, .0098}, {.0080, .0058, .0051, .0045, .0040, .0035, .0026, .0028, .0033, .0040, .0061}}
	for i, years := range []int{1, 3} {
		date := inflGridDate(t, base.Day(), base.Month(), base.Year()+years)
		strikes, vols, e := surface.DSlice(date)
		if e != nil || len(strikes) != 11 || len(vols) != 11 {
			t.Fatal(strikes, vols, e)
		}
		for j, x := range vols {
			inflGridClose(t, strikes[j], inflGridAllStrikes[j], 1e-12, nil)
			inflGridClose(t, x, expected[i][j], 1e-4, nil)
		}
	}
	x, e := surface.MinStrike()
	inflGridClose(t, x, -.01, 1e-12, e)
	x, e = surface.MaxStrike()
	inflGridClose(t, x, .05, 1e-12, e)
	max, e := surface.MaxDate()
	if e != nil || max != inflGridDate(t, 23, 11, 2037) {
		t.Fatal(max, e)
	}
	year1 := inflGridDate(t, base.Day(), base.Month(), base.Year()+1)
	quote, e := f.calendar.Advance(year1, 3, Months, Unadjusted, false)
	if e != nil {
		t.Fatal(e)
	}
	x, e = surface.Volatility(quote, .03)
	inflGridClose(t, x, expected[0][7], 1e-4, e)
	if e = f.prices.Close(); e != nil {
		t.Fatal(e)
	}
	if e = f.index.Close(); e != nil {
		t.Fatal(e)
	}
	_, vols, e := surface.DSlice(year1)
	if e != nil || len(vols) != 11 {
		t.Fatal(vols, e)
	}
	inflGridClose(t, vols[7], expected[0][7], 1e-4, nil)
}
