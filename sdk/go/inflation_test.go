package itofin

import (
	"math"
	"testing"
)

func inflationDate(t *testing.T, d, m, y int) Date {
	t.Helper()
	v, e := NewDate(d, m, y)
	return creditMust(t, v, e)
}

// QuantLib testZeroIndex table: published monthly UK RPI figures.
func TestInflationPublishedFixingsAndUnderlyingLifetime(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	settings0, e := s.NewSettings()
	settings := creditMust(t, settings0, e)
	if e = settings.SetEvaluationDate(inflationDate(t, 13, 9, 2007)); e != nil {
		t.Fatal(e)
	}
	for _, factory := range []struct {
		name string
		new  func(*Settings) (*ZeroInflationIndex, error)
	}{{"UK RPI", s.NewUKRPI}, {"UK HICP", s.NewUKHICP}, {"EU HICP", s.NewEUHICP}} {
		i0, e := factory.new(settings)
		i := creditMust(t, i0, e)
		name, e := i.Name()
		if e != nil || name != factory.name {
			t.Fatalf("name %q: %v", name, e)
		}
		if e = i.AddFixing(inflationDate(t, 17, 7, 2007), 207.3); e != nil {
			t.Fatal(e)
		}
		for _, day := range []int{1, 17, 31} {
			v, e := i.Fixing(inflationDate(t, day, 7, 2007), false)
			if e != nil || math.Abs(v-207.3) > 1e-12 {
				t.Fatalf("published fixing %g: %v", v, e)
			}
		}
		last, e := i.LastFixingDate()
		if e != nil || last.Serial() != inflationDate(t, 1, 7, 2007).Serial() {
			t.Fatalf("last fixing %v: %v", last, e)
		}
		future := inflationDate(t, 1, 6, 2010)
		needs, e := i.NeedsForecast(future)
		if e != nil || !needs {
			t.Fatalf("needs forecast %v: %v", needs, e)
		}
		if _, e = i.Fixing(future, false); e == nil {
			t.Fatal("unlinked forecast accepted")
		}
	}
	zero0, e := s.NewUKRPI(settings)
	zero := creditMust(t, zero0, e)
	if e = zero.AddFixing(inflationDate(t, 1, 7, 2006), 200); e != nil {
		t.Fatal(e)
	}
	yoy0, e := s.NewYoYInflationIndexFromUnderlying(zero)
	yoy := creditMust(t, yoy0, e)
	if e = zero.Close(); e != nil {
		t.Fatal(e)
	}
	ratio, e := yoy.Ratio()
	if e != nil || !ratio {
		t.Fatalf("ratio %v: %v", ratio, e)
	}
	fixing, e := yoy.Fixing(inflationDate(t, 1, 7, 2007), false)
	if e != nil || math.Abs(fixing-(207.3/200-1)) > 1e-12 {
		t.Fatalf("ratio fixing %.15g: %v", fixing, e)
	}
	retained0, e := yoy.UnderlyingIndex()
	retained := creditMust(t, retained0, e)
	if retained == nil {
		t.Fatal("underlying lost")
	}
	if _, e = retained.Name(); e != nil {
		t.Fatal(e)
	}
	quoted0, e := s.NewYoYInflationIndex(YoYInflationIndexConfig{FamilyName: "TestYoY", RegionName: "UK", RegionCode: "UK", Frequency: Monthly, AvailabilityLag: Period{1, Months}, CurrencyName: "Pound", CurrencyCode: "GBP", CurrencyNumericCode: 826, CurrencySymbol: "£", CurrencyFractionSymbol: "p", CurrencyFractionsPerUnit: 100, Settings: settings})
	quoted := creditMust(t, quoted0, e)
	if e = quoted.AddFixing(inflationDate(t, 1, 7, 2007), .025); e != nil {
		t.Fatal(e)
	}
	v, e := quoted.Fixing(inflationDate(t, 17, 7, 2007), false)
	if e != nil || math.Abs(v-.025) > 1e-12 {
		t.Fatalf("quoted fixing: %g %v", v, e)
	}
	under, e := quoted.UnderlyingIndex()
	if e != nil || under != nil {
		t.Fatalf("quoted underlying: %v %v", under, e)
	}
}
func TestInflationCurvePeriodQuantizationAndSeasonality(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	dc0, e := s.Thirty360BondBasis()
	dc := creditMust(t, dc0, e)
	dates := []Date{inflationDate(t, 1, 7, 2007), inflationDate(t, 1, 9, 2007), inflationDate(t, 1, 1, 2008)}
	rates := []float64{.02, .022, .025}
	cfg := InflationCurveConfig{ReferenceDate: inflationDate(t, 13, 8, 2007), Frequency: Monthly, DayCounter: dc}
	curve0, e := s.NewInterpolatedZeroInflationCurve(cfg, dates, rates)
	curve := creditMust(t, curve0, e)
	times, e := curve.Times()
	if e != nil || math.Abs(times[0]+42.0/360) > 1e-12 {
		t.Fatalf("base time %v: %v", times, e)
	}
	mid := inflationDate(t, 15, 9, 2007)
	v, e := curve.ZeroRateDate(mid, false)
	if e != nil || math.Abs(v-.022) > 1e-12 {
		t.Fatalf("date rate %g: %v", v, e)
	}
	v, e = curve.ZeroRate(32.0/360, false)
	if e != nil || math.Abs(v-.02235) > 1e-12 {
		t.Fatalf("time rate %g: %v", v, e)
	}
	if _, e = curve.ZeroRateDate(inflationDate(t, 20, 6, 2007), false); e == nil {
		t.Fatal("date before base accepted")
	}
	factors := []float64{1, 1.01, 1.02, 1.03, 1.02, 1.01, 1, .99, .98, .97, .98, .99}
	season0, e := s.NewMultiplicativePriceSeasonality(dates[0], Monthly, factors)
	season := creditMust(t, season0, e)
	read, e := season.SeasonalityFactors()
	if e != nil || len(read) != 12 || read[3] != 1.03 {
		t.Fatalf("factors %v: %v", read, e)
	}
	factor, e := season.SeasonalityFactor(inflationDate(t, 1, 10, 2007))
	if e != nil || factor != 1.03 {
		t.Fatalf("factor %g: %v", factor, e)
	}
	if e = curve.SetSeasonality(season); e != nil {
		t.Fatal(e)
	}
	if e = season.Close(); e != nil {
		t.Fatal(e)
	}
	has, e := curve.HasSeasonality()
	if e != nil || !has {
		t.Fatalf("seasonality lost: %v", e)
	}
	if _, e = curve.ZeroRateDate(mid, false); e != nil {
		t.Fatal(e)
	}
	if e = curve.SetSeasonality(nil); e != nil {
		t.Fatal(e)
	}
	has, e = curve.HasSeasonality()
	if e != nil || has {
		t.Fatalf("seasonality not cleared: %v", e)
	}
	yoy0, e := s.NewInterpolatedYoYInflationCurve(cfg, dates, rates)
	yoy := creditMust(t, yoy0, e)
	base, e := yoy.BaseRate()
	if e != nil || base != .02 {
		t.Fatalf("YoY base %g: %v", base, e)
	}
	for j, d := range dates {
		r, e := yoy.YoYRateDate(d, false)
		if e != nil || math.Abs(r-rates[j]) > 1e-12 {
			t.Fatalf("YoY rate %g: %v", r, e)
		}
	}
}
func TestZeroInflationBootstrapHelperDatesAndQuoteUpdate(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	today := inflationDate(t, 13, 8, 2007)
	settings0, e := s.NewSettings()
	settings := creditMust(t, settings0, e)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	index0, e := s.NewUKRPI(settings)
	index := creditMust(t, index0, e)
	for j, v := range []float64{204.4, 205.4, 206.2, 207.3} {
		if e = index.AddFixing(inflationDate(t, 1, j+4, 2007), v); e != nil {
			t.Fatal(e)
		}
	}
	dc0, e := s.Thirty360BondBasis()
	dc := creditMust(t, dc0, e)
	cal0, e := s.UnitedKingdom()
	cal := creditMust(t, cal0, e)
	q0, e := s.NewSimpleQuote(.03)
	q := creditMust(t, q0, e)
	helper0, e := s.NewZeroCouponInflationSwapHelper(InflationHelperConfig{Quote: q, SwapObservationLag: Period{3, Months}, Maturity: inflationDate(t, 13, 8, 2008), Calendar: cal, PaymentConvention: ModifiedFollowing, DayCounter: dc, ObservationInterpolation: CpiFlat, Settings: settings}, index)
	helper := creditMust(t, helper0, e)

	linearConfig := InflationHelperConfig{Quote: q, SwapObservationLag: Period{3, Months}, Maturity: inflationDate(t, 13, 8, 2008), Calendar: cal, PaymentConvention: ModifiedFollowing, DayCounter: dc, ObservationInterpolation: CpiLinear, Settings: settings}
	linear0, e := s.NewZeroCouponInflationSwapHelper(linearConfig, index)
	linear := creditMust(t, linear0, e)
	defaultPillar, e := linear.PillarDate()
	if e != nil || defaultPillar.Serial() != inflationDate(t, 1, 5, 2008).Serial() {
		t.Fatalf("default LastRelevantDate pillar %v: %v", defaultPillar, e)
	}
	far := MaturityDate
	linearConfig.Pillar = &far
	far0, e := s.NewZeroCouponInflationSwapHelper(linearConfig, index)
	farHelper := creditMust(t, far0, e)
	farPillar, e := farHelper.PillarDate()
	if e != nil || farPillar.Serial() != inflationDate(t, 1, 6, 2008).Serial() {
		t.Fatalf("explicit MaturityDate pillar %v: %v", farPillar, e)
	}
	pillar, e := helper.PillarDate()
	if e != nil || pillar.Serial() != inflationDate(t, 1, 5, 2008).Serial() {
		t.Fatalf("pillar %v: %v", pillar, e)
	}
	obs, e := helper.InflationFixingDate()
	if e != nil || obs.Serial() != inflationDate(t, 13, 5, 2008).Serial() {
		t.Fatalf("observation %v: %v", obs, e)
	}
	curve0, e := s.NewPiecewiseZeroInflationCurve(InflationCurveConfig{ReferenceDate: today, BaseDate: inflationDate(t, 1, 7, 2007), Frequency: Monthly, DayCounter: dc}, []*ZeroInflationHelper{helper})
	curve := creditMust(t, curve0, e)
	if e = curve.Calculate(); e != nil {
		t.Fatal(e)
	}
	before, e := curve.ZeroRateDate(pillar, false)
	if e != nil {
		t.Fatal(e)
	}
	if e = q.SetValue(.04); e != nil {
		t.Fatal(e)
	}
	after, e := curve.ZeroRateDate(pillar, false)
	if e != nil || after <= before {
		t.Fatalf("bootstrap quote did not update: %g -> %g: %v", before, after, e)
	}
	if e = helper.Close(); e != nil {
		t.Fatal(e)
	}
	if e = q.Close(); e != nil {
		t.Fatal(e)
	}
	if _, e = curve.Nodes(); e != nil {
		t.Fatal(e)
	}
}

// The first two quoted swaps and published history of QuantLib testYYTermStructure.
func TestYoYInflationBootstrapRelinkingAndRetainedCurves(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	today := inflationDate(t, 13, 8, 2007)
	settings0, e := s.NewSettings()
	settings := creditMust(t, settings0, e)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	zero0, e := s.NewUKRPI(settings)
	zero := creditMust(t, zero0, e)
	values := []float64{189.9, 189.9, 189.6, 190.5, 191.6, 192, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1, 193.4, 194.2, 195, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2, 207.3}
	for j, v := range values {
		if e = zero.AddFixing(inflationDate(t, 1, j%12+1, 2005+j/12), v); e != nil {
			t.Fatal(e)
		}
	}
	index0, e := s.NewYoYInflationIndexFromUnderlying(zero)
	index := creditMust(t, index0, e)
	dc0, e := s.Thirty360BondBasis()
	dc := creditMust(t, dc0, e)
	nomdc0, e := s.Actual360()
	nomdc := creditMust(t, nomdc0, e)
	nominal0, e := s.NewFlatForward(today, .05, nomdc)
	nominal := creditMust(t, nominal0, e)
	cal0, e := s.UnitedKingdom()
	cal := creditMust(t, cal0, e)
	helpers := make([]*YoYInflationHelper, 2)
	quotes := make([]*SimpleQuote, 2)
	for j := range helpers {
		q0, e := s.NewSimpleQuote(.0295)
		quotes[j] = creditMust(t, q0, e)
		h0, e := s.NewYearOnYearInflationSwapHelper(InflationHelperConfig{Quote: quotes[j], SwapObservationLag: Period{2, Months}, Maturity: inflationDate(t, 13, 8, 2008+j), Calendar: cal, PaymentConvention: ModifiedFollowing, DayCounter: dc, ObservationInterpolation: CpiFlat, Settings: settings}, index, nominal)
		helpers[j] = creditMust(t, h0, e)
	}
	base := inflationDate(t, 1, 7, 2007)
	curve0, e := s.NewPiecewiseYoYInflationCurve(InflationCurveConfig{ReferenceDate: today, BaseDate: base, BaseYoYRate: .0295, Frequency: Monthly, DayCounter: dc}, helpers)
	curve := creditMust(t, curve0, e)
	if e = curve.Calculate(); e != nil {
		t.Fatal(e)
	}
	dates, e := curve.Dates()
	if e != nil || len(dates) != 3 || dates[0].Serial() != base.Serial() {
		t.Fatalf("YoY bootstrap nodes %v: %v", dates, e)
	}
	if e = index.LinkTo(curve); e != nil {
		t.Fatal(e)
	}
	future := inflationDate(t, 1, 6, 2009)
	before, e := index.Fixing(future, false)
	if e != nil {
		t.Fatal(e)
	}
	if e = quotes[1].SetValue(.035); e != nil {
		t.Fatal(e)
	}
	after, e := index.Fixing(future, false)
	if e != nil || after <= before {
		t.Fatalf("YoY quote update %g -> %g: %v", before, after, e)
	}
	if e = curve.Close(); e != nil {
		t.Fatal(e)
	}
	if e = zero.Close(); e != nil {
		t.Fatal(e)
	}
	retained, e := index.Fixing(future, false)
	if e != nil || retained != after {
		t.Fatalf("retained forecast %g: %v", retained, e)
	}
	replacement0, e := s.NewInterpolatedYoYInflationCurve(InflationCurveConfig{ReferenceDate: today, Frequency: Monthly, DayCounter: dc}, []Date{base, inflationDate(t, 1, 7, 2010)}, []float64{.04, .04})
	replacement := creditMust(t, replacement0, e)
	if e = index.LinkTo(replacement); e != nil {
		t.Fatal(e)
	}
	relinked, e := index.Fixing(future, false)
	if e != nil || math.Abs(relinked-.04) > 1e-12 {
		t.Fatalf("relinked forecast %g: %v", relinked, e)
	}
}

func TestInflationIndexMetadataPreservesEmbeddedNUL(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	settings0, e := s.NewSettings()
	settings := creditMust(t, settings0, e)
	index0, e := s.NewYoYInflationIndex(YoYInflationIndexConfig{FamilyName: "F\x00amily", RegionName: "N\x00orth", RegionCode: "N", Frequency: Monthly, AvailabilityLag: Period{1, Months}, CurrencyName: "P\x00ound", CurrencyCode: "GBP", CurrencyNumericCode: 826, CurrencySymbol: "£", CurrencyFractionSymbol: "p", CurrencyFractionsPerUnit: 100, Settings: settings})
	index := creditMust(t, index0, e)
	name, e := index.Name()
	if e != nil || name != "N\x00orth F\x00amily" {
		t.Fatalf("metadata was truncated: %q %v", name, e)
	}
}
