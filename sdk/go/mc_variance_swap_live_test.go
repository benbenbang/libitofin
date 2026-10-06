package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
	"time"
)

func TestMCVarianceSwapNativeLiveSnapshots(t *testing.T) {
	data, err := os.ReadFile("testdata/mc-variance-swap/live_updates.json")
	pricingOK(t, err)
	var fixture struct {
		Inputs  mcVarianceInputs `json:"inputs"`
		Native  mcVarianceNative `json:"native"`
		Updates []struct {
			Market struct {
				Spot     float64 `json:"spot"`
				Rate     float64 `json:"risk_free_rate"`
				Dividend float64 `json:"dividend_yield"`
				Vol      float64 `json:"volatility"`
			} `json:"market"`
			Native mcVarianceNative `json:"native"`
		} `json:"updates"`
	}
	pricingOK(t, json.Unmarshal(data, &fixture))
	if len(fixture.Updates) != 4 {
		t.Fatal("native live fixture coverage")
	}
	a := fixture.Inputs
	m := newVarianceSwapMarket(t)
	parsed := pricingMust(time.Parse("2006-01-02", a.EvaluationDate))
	m.today = pricingMust(NewDate(parsed.Day(), int(parsed.Month()), parsed.Year()))
	m.maturity = pricingMust(m.today.AddDays(int64(a.MaturityDays)))
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	dc := pricingMust(m.s.Actual365Fixed())
	m.risk = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[0], dc))
	m.dividend = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[1], dc))
	m.vol = pricingMust(m.s.NewBlackConstantVolFromQuote(m.today, m.quotes[2], dc, nil))
	m.swap = pricingMust(m.s.NewVarianceSwap(VarianceSwapConfig{PositionLong, a.Strike, a.Notional, m.today, m.maturity, m.settings}))
	config := MCVarianceSwapConfig{Steps: a.Steps, StepsPerYear: a.StepsPerYear, Samples: a.Samples, AbsoluteTolerance: a.Tolerance, MaxSamples: a.MaxSamples, Seed: a.Seed}
	attachSpot := func(spot float64) {
		process := pricingMust(m.s.NewBlackScholesProcessFromCurves(spot, m.risk, m.dividend, m.vol))
		engine := pricingMust(m.s.NewMCVarianceSwapEngine(process, config))
		pricingOK(t, m.swap.SetMCEngine(engine))
		pricingOK(t, engine.Close())
		pricingOK(t, process.Close())
	}
	check := func(native mcVarianceNative) {
		compare := func(got, want, abs float64) { varianceSwapNear(t, got, want, math.Max(abs, 3e-12*math.Abs(want))) }
		compare(pricingMust(m.swap.Variance()), native.Variance, 2e-14)
		compare(pricingMust(m.swap.NPV()), native.NPV, 2e-9)
		compare(pricingMust(m.swap.VarianceError()), native.VarianceError, 2e-14)
		compare(pricingMust(m.swap.ErrorEstimate()), native.ErrorEstimate, 2e-9)
		if pricingMust(m.swap.Samples()) != native.Samples {
			t.Fatal("native live actual samples")
		}
	}
	spot := a.Spot
	attachSpot(spot)
	check(fixture.Native)
	for _, u := range fixture.Updates {
		pricingOK(t, m.quotes[0].SetValue(u.Market.Rate))
		pricingOK(t, m.quotes[1].SetValue(u.Market.Dividend))
		pricingOK(t, m.quotes[2].SetValue(u.Market.Vol))
		if u.Market.Spot != spot {
			attachSpot(u.Market.Spot)
			spot = u.Market.Spot
		}
		if pricingMust(m.swap.IsCalculated()) {
			t.Fatal("native live update not invalidated")
		}
		check(u.Native)
	}
}
