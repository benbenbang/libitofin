package itofin

import (
	"math"
	"testing"
)

func TestInflationCurveMetadataAndDetachedNodes(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(13, 8, 2007))
	dates := []Date{pricingMust(NewDate(1, 7, 2007)), pricingMust(NewDate(1, 9, 2007)), pricingMust(NewDate(1, 1, 2008))}
	rates := []float64{.02, .022, .025}
	wantTimes := []float64{-42.0 / 360, 18.0 / 360, 138.0 / 360}
	dc := pricingMust(s.Thirty360BondBasis())
	cfg := InflationCurveConfig{ReferenceDate: ref, Frequency: Monthly, DayCounter: dc}
	zero := pricingMust(s.NewInterpolatedZeroInflationCurve(cfg, dates, rates))
	yoy := pricingMust(s.NewInterpolatedYoYInflationCurve(cfg, dates, rates))
	for _, curve := range []interface {
		BaseDate() (Date, error)
		Frequency() (Frequency, error)
		Dates() ([]Date, error)
		Times() ([]float64, error)
		Nodes() ([]InflationNode, error)
	}{zero, yoy} {
		if pricingMust(curve.BaseDate()) != dates[0] || pricingMust(curve.Frequency()) != Monthly {
			t.Fatal("base date or frequency changed")
		}
		gotDates := pricingMust(curve.Dates())
		times := pricingMust(curve.Times())
		nodes := pricingMust(curve.Nodes())
		if len(gotDates) != len(dates) || len(times) != len(dates) || len(nodes) != len(dates) {
			t.Fatal("inflation node shape changed")
		}
		for i, date := range dates {
			if gotDates[i] != date || nodes[i].Date != date {
				t.Fatalf("inflation node %d date changed", i)
			}
			pricingNear(t, times[i], wantTimes[i], 1e-12)
			pricingNear(t, nodes[i].Time, wantTimes[i], 1e-12)
			pricingNear(t, nodes[i].Rate, rates[i], 1e-12)
		}
		gotDates[0], nodes[0].Date = Date{}, Date{}
		times[0], nodes[0].Rate = -100, -100
		if pricingMust(curve.Dates())[0] != dates[0] || pricingMust(curve.Nodes())[0].Date != dates[0] {
			t.Fatal("returned dates mutate native state")
		}
		pricingNear(t, pricingMust(curve.Times())[0], wantTimes[0], 1e-12)
		pricingNear(t, pricingMust(curve.Nodes())[0].Rate, rates[0], 1e-12)
	}
	pricingNear(t, pricingMust(yoy.YoYRate(32.0/360, false)), .02235, 1e-12)
	season := pricingMust(s.NewMultiplicativePriceSeasonality(dates[0], Monthly, []float64{1, 1.01, 1.02, 1.03, 1.02, 1.01, 1, .99, .98, .97, .98, .99}))
	if pricingMust(season.SeasonalityBaseDate()) != dates[0] || pricingMust(season.Frequency()) != Monthly {
		t.Fatal("seasonality base date or frequency changed")
	}
}

func TestZeroInflationRelinkingRetainsIndependentForecast(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	ref := pricingMust(NewDate(13, 8, 2007))
	base := pricingMust(NewDate(1, 7, 2007))
	future := pricingMust(NewDate(1, 7, 2009))
	pricingOK(t, settings.SetEvaluationDate(ref))
	index := pricingMust(s.NewUKRPI(settings))
	pricingOK(t, index.AddFixing(base, 207.3))
	dc := pricingMust(s.Thirty360BondBasis())
	for _, rate := range []float64{.02, .04} {
		curve := pricingMust(s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: ref, Frequency: Monthly, DayCounter: dc}, []Date{base, future}, []float64{rate, rate}))
		pricingOK(t, index.LinkTo(curve))
		pricingOK(t, curve.Close())
		pricingNear(t, pricingMust(index.Fixing(future, false)), 207.3*math.Pow(1+rate, 2), 1e-12)
	}
}
