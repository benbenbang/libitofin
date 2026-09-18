package itofin

import (
	"errors"
	"math"
	"testing"
)

func TestFlatHazardAnalyticQueriesAndRetainedQuote(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(1, 1, 2025))
	end := pricingMust(NewDate(1, 1, 2027))
	dc := pricingMust(s.Actual365Fixed())
	quote := pricingMust(s.NewSimpleQuote(.02))
	curve := pricingMust(s.NewFlatHazardRate(FlatHazardConfig{ReferenceDate: ref, Quote: quote, DayCounter: dc}))
	check := func(rate float64) {
		t.Helper()
		survival := math.Exp(-rate * 2)
		for _, query := range []struct {
			fn   func() (float64, error)
			want float64
		}{
			{func() (float64, error) { return curve.SurvivalProbability(2, false) }, survival},
			{func() (float64, error) { return curve.SurvivalProbabilityDate(end, false) }, survival},
			{func() (float64, error) { return curve.DefaultProbability(2, false) }, 1 - survival},
			{func() (float64, error) { return curve.DefaultProbabilityDate(end, false) }, 1 - survival},
			{func() (float64, error) { return curve.DefaultDensity(2, false) }, rate * survival},
			{func() (float64, error) { return curve.DefaultDensityDate(end, false) }, rate * survival},
			{func() (float64, error) { return curve.HazardRate(2, false) }, rate},
			{func() (float64, error) { return curve.HazardRateDate(end, false) }, rate},
		} {
			pricingNear(t, pricingMust(query.fn()), query.want, 1e-12)
		}
	}
	check(.02)
	pricingOK(t, quote.SetValue(.07))
	check(.07)
	pricingOK(t, quote.Close())
	pricingOK(t, dc.Close())
	check(.07)
	pricingOK(t, s.Close())
	if _, err := curve.DefaultDensityDate(end, false); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
}
