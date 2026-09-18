package itofin

import (
	"math"
	"testing"
)

func TestCreditCompletionHazardArraysAndNonflatISDA(t *testing.T) {
	f := creditCompletionSetup(t)
	oracle := creditCompletionRead(t)
	dates := []Date{f.today, creditCompletionDate("2026-07-15"), creditCompletionDate("2026-10-15"), creditCompletionDate("2027-06-15"), creditCompletionDate("2028-06-15"), creditCompletionDate("2030-06-15")}
	rates := []float64{.01, .012, .018, .025, .03, .035}
	discounts := []float64{1, .998, .988, .967, .923, .84}
	hazard := pricingMust(f.s.NewInterpolatedHazardRateCurve(dates, rates, f.curveDC))
	discount := pricingMust(f.s.NewDiscountCurve(NodeCurveConfig{Dates: dates, Values: discounts, DayCounter: f.curveDC}))
	returnedDates := pricingMust(hazard.Dates())
	returnedRates := pricingMust(hazard.HazardRates())
	if len(returnedDates) != len(dates) || len(returnedRates) != len(rates) {
		t.Fatal("hazard array sizes")
	}
	for i := range dates {
		if returnedDates[i] != dates[i] || returnedRates[i] != rates[i] {
			t.Fatalf("hazard node %d", i)
		}
	}
	integratedHazard := 0.
	for i := 1; i < len(dates); i++ {
		integratedHazard += rates[i] * float64(dates[i].Serial()-dates[i-1].Serial()) / 365
	}
	wantSurvival := math.Exp(-integratedHazard)
	pricingNear(t, pricingMust(hazard.SurvivalProbabilityDate(dates[len(dates)-1], false)), wantSurvival, 1e-12)
	returnedDates[1], returnedRates[1] = f.end, 99
	dates[1], rates[1] = f.end, 98
	freshDates := pricingMust(hazard.Dates())
	freshRates := pricingMust(hazard.HazardRates())
	if freshDates[1] != creditCompletionDate("2026-07-15") || freshRates[1] != .012 {
		t.Fatal("hazard arrays alias input or native state")
	}
	pricingNear(t, pricingMust(hazard.SurvivalProbabilityDate(dates[len(dates)-1], false)), wantSurvival, 1e-12)
	cds := pricingMust(f.s.NewCreditDefaultSwap(f.config))
	observed := make(map[string]float64)
	if len(oracle.ISDA) != 4 {
		t.Fatal("ISDA convention matrix is incomplete")
	}
	for _, want := range oracle.ISDA {
		t.Run(want.Fix+"/"+want.Forwards, func(t *testing.T) {
			fix, forwards := NoFix, FlatForwards
			if want.Fix == "Taylor" {
				fix = Taylor
			}
			if want.Forwards == "Piecewise" {
				forwards = PiecewiseForwards
			}
			engine := pricingMust(f.s.NewIsdaCdsEngine(CdsEngineConfig{Probability: hazard, Discount: discount, Settings: f.settings, Recovery: .4, NumericalFix: &fix, Forwards: &forwards}))
			pricingOK(t, cds.SetIsdaEngine(engine))
			creditCompletionCheck(t, cds, want)
			observed[want.Fix+want.Forwards] = pricingMust(cds.NPV())
		})
	}
	if math.Abs(observed["NoFixFlat"]-observed["NoFixPiecewise"]) < 1 {
		t.Fatal("non-flat fixture does not distinguish forwards conventions")
	}
}
