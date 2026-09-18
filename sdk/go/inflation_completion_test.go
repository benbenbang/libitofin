package itofin

import (
	"fmt"
	"testing"
)

func TestInflationCompletionIndexRepresentations(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	for _, tc := range []struct {
		name string
		new  func(*Settings) (*ZeroInflationIndex, error)
	}{{"UK RPI", s.NewUKRPI}, {"UK HICP", s.NewUKHICP}, {"EU HICP", s.NewEUHICP}} {
		zero := pricingMust(tc.new(settings))
		if got := fmt.Sprint(zero); got != "ZeroInflationIndex("+tc.name+")" {
			t.Fatalf("zero representation %q", got)
		}
		yoy := pricingMust(s.NewYoYInflationIndexFromUnderlying(zero))
		pricingOK(t, zero.Close())
		region, family := "UK", "RPI"
		switch tc.name {
		case "UK HICP":
			family = "HICP"
		case "EU HICP":
			region, family = "EU", "HICP"
		}
		name := region + " YYR_" + family
		if pricingMust(yoy.Name()) != name || fmt.Sprint(yoy) != "YoYInflationIndex("+name+")" || !pricingMust(yoy.Ratio()) {
			t.Fatalf("ratio metadata lost after underlying close: %s", yoy)
		}
	}
	quoted := pricingMust(s.NewYoYInflationIndex(YoYInflationIndexConfig{FamilyName: "Quoted RPI", RegionName: "UK", RegionCode: "UK", Frequency: Monthly, AvailabilityLag: Period{1, Months}, CurrencyName: "Pound", CurrencyCode: "GBP", CurrencyNumericCode: 826, CurrencySymbol: "£", CurrencyFractionSymbol: "p", CurrencyFractionsPerUnit: 100, Settings: settings}))
	if fmt.Sprint(quoted) != "YoYInflationIndex(UK Quoted RPI)" || pricingMust(quoted.Ratio()) {
		t.Fatalf("quoted metadata: %s", quoted)
	}
}

type inflationCompletionMarket struct {
	s        *Session
	settings *Settings
	zero     *ZeroInflationIndex
	yoy      *YoYInflationIndex
	dc       *DayCounter
	cal      *Calendar
	nominal  *YieldTermStructure
}

func newInflationCompletionMarket(t *testing.T) inflationCompletionMarket {
	t.Helper()
	s := pricingMust(NewSession())
	t.Cleanup(func() { s.Close() })
	settings := pricingMust(s.NewSettings())
	today := inflationDate(t, 13, 8, 2007)
	pricingOK(t, settings.SetEvaluationDate(today))
	zero := pricingMust(s.NewUKRPI(settings))
	for i, value := range []float64{204.4, 205.4, 206.2, 207.3} {
		pricingOK(t, zero.AddFixing(inflationDate(t, 1, i+4, 2007), value))
	}
	yoy := pricingMust(s.NewYoYInflationIndex(YoYInflationIndexConfig{FamilyName: "YY_RPI", RegionName: "UK", RegionCode: "UK", Frequency: Monthly, AvailabilityLag: Period{1, Months}, CurrencyName: "Pound", CurrencyCode: "GBP", CurrencyNumericCode: 826, CurrencySymbol: "£", CurrencyFractionSymbol: "p", CurrencyFractionsPerUnit: 100, Settings: settings}))
	dc := pricingMust(s.Thirty360BondBasis())
	cal := pricingMust(s.UnitedKingdom())
	nominal := pricingMust(s.NewFlatForward(today, .05, pricingMust(s.Actual360())))
	return inflationCompletionMarket{s, settings, zero, yoy, dc, cal, nominal}
}

func (m inflationCompletionMarket) helperConfig(t *testing.T, day, year int, interpolation CpiInterpolationType, pillar *Pillar) InflationHelperConfig {
	t.Helper()
	return InflationHelperConfig{Quote: pricingMust(m.s.NewSimpleQuote(.0295)), SwapObservationLag: Period{2, Months}, Maturity: inflationDate(t, day, 8, year), Calendar: m.cal, PaymentConvention: ModifiedFollowing, DayCounter: m.dc, ObservationInterpolation: interpolation, Settings: m.settings, Pillar: pillar}
}

func TestInflationCompletionHelperDates(t *testing.T) {
	m := newInflationCompletionMarket(t)
	maturity := MaturityDate
	for _, tc := range []struct {
		name          string
		day           int
		interpolation CpiInterpolationType
		pillar        *Pillar
		pillarMonth   int
		latestMonth   int
	}{
		{"flat", 13, CpiFlat, nil, 6, 6},
		{"linear-early-weight", 13, CpiLinear, nil, 6, 7},
		{"linear-late-weight", 20, CpiLinear, nil, 7, 7},
		{"linear-maturity-pillar", 13, CpiLinear, &maturity, 7, 7},
	} {
		t.Run(tc.name, func(t *testing.T) {
			cfg := m.helperConfig(t, tc.day, 2008, tc.interpolation, tc.pillar)
			zero := pricingMust(m.s.NewZeroCouponInflationSwapHelper(cfg, m.zero))
			yoy := pricingMust(m.s.NewYearOnYearInflationSwapHelper(cfg, m.yoy, m.nominal))
			for name, helper := range map[string]interface {
				PillarDate() (Date, error)
				LatestDate() (Date, error)
			}{"zero": zero, "yoy": yoy} {
				if got := pricingMust(helper.LatestDate()); got != inflationDate(t, 1, tc.latestMonth, 2008) {
					t.Fatalf("%s latest date %v", name, got)
				}
				if got := pricingMust(helper.PillarDate()); got != inflationDate(t, 1, tc.pillarMonth, 2008) {
					t.Fatalf("%s pillar date %v", name, got)
				}
			}
		})
	}
}

func TestInflationCompletionPiecewiseDetachedOutputs(t *testing.T) {
	m := newInflationCompletionMarket(t)
	zeroHelpers := make([]*ZeroInflationHelper, 2)
	yoyHelpers := make([]*YoYInflationHelper, 2)
	for i := range zeroHelpers {
		cfg := m.helperConfig(t, 13, 2008+i, CpiFlat, nil)
		zeroHelpers[i] = pricingMust(m.s.NewZeroCouponInflationSwapHelper(cfg, m.zero))
		yoyHelpers[i] = pricingMust(m.s.NewYearOnYearInflationSwapHelper(cfg, m.yoy, m.nominal))
	}
	base := inflationDate(t, 1, 7, 2007)
	cfg := InflationCurveConfig{ReferenceDate: inflationDate(t, 13, 8, 2007), BaseDate: base, BaseYoYRate: .0295, Frequency: Monthly, DayCounter: m.dc}
	zero := pricingMust(m.s.NewPiecewiseZeroInflationCurve(cfg, zeroHelpers))
	yoy := pricingMust(m.s.NewPiecewiseYoYInflationCurve(cfg, yoyHelpers))
	wantDates := []Date{base, inflationDate(t, 1, 6, 2008), inflationDate(t, 1, 6, 2009)}
	wantTimes := []float64{-42.0 / 360, 288.0 / 360, 648.0 / 360}
	wantZeroRates := []float64{.026250783298904422, .026250783298904422, .027944745304696286}
	for name, curve := range map[string]interface {
		Dates() ([]Date, error)
		Times() ([]float64, error)
		Nodes() ([]InflationNode, error)
		Close() error
	}{"zero": zero, "yoy": yoy} {
		t.Run(name, func(t *testing.T) {
			dates := pricingMust(curve.Dates())
			times := pricingMust(curve.Times())
			nodes := pricingMust(curve.Nodes())
			if len(dates) != 3 || len(times) != 3 || len(nodes) != 3 {
				t.Fatalf("curve output lengths: %d, %d, %d", len(dates), len(times), len(nodes))
			}
			for i := range wantDates {
				if dates[i] != wantDates[i] || nodes[i].Date != wantDates[i] {
					t.Fatalf("node %d date: %v, %v", i, dates[i], nodes[i].Date)
				}
				pricingNear(t, times[i], wantTimes[i], 1e-12)
				pricingNear(t, nodes[i].Time, wantTimes[i], 1e-12)
				if name == "yoy" {
					pricingNear(t, nodes[i].Rate, .0295, 1e-12)
				} else {
					pricingNear(t, nodes[i].Rate, wantZeroRates[i], 1e-12)
				}
			}
			original := append([]InflationNode(nil), nodes...)
			for i := range dates {
				dates[i], times[i], nodes[i] = Date{}, -999, InflationNode{}
			}
			freshDates := pricingMust(curve.Dates())
			freshTimes := pricingMust(curve.Times())
			freshNodes := pricingMust(curve.Nodes())
			pricingOK(t, curve.Close())
			for i := range original {
				if freshDates[i] != original[i].Date || freshTimes[i] != original[i].Time || freshNodes[i] != original[i] {
					t.Fatalf("native output mutated or snapshot invalid after close at node %d", i)
				}
			}
		})
	}
}
