package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
	"time"
)

type mcVarianceInputs struct {
	EvaluationDate string    `json:"evaluation_date"`
	MaturityDays   int       `json:"maturity_days"`
	Spot           float64   `json:"spot"`
	Rate           float64   `json:"risk_free_rate"`
	Dividend       float64   `json:"dividend_yield"`
	Vol            float64   `json:"volatility"`
	Strike         float64   `json:"variance_strike"`
	Notional       float64   `json:"notional"`
	Position       string    `json:"position"`
	VolatilityKind string    `json:"volatility_kind"`
	CurveDays      []int     `json:"curve_days"`
	CurveVols      []float64 `json:"curve_vols"`
	Steps          uint      `json:"steps"`
	StepsPerYear   uint      `json:"steps_per_year"`
	Samples        uint      `json:"samples"`
	Tolerance      float64   `json:"tolerance"`
	MaxSamples     uint      `json:"max_samples"`
	Seed           uint64    `json:"seed"`
}

type mcVarianceNative struct {
	Variance      float64 `json:"variance"`
	NPV           float64 `json:"npv"`
	VarianceError float64 `json:"variance_error"`
	ErrorEstimate float64 `json:"error_estimate"`
	Samples       uint    `json:"samples"`
}

func TestMCVarianceSwapNativeQuantLibOracle(t *testing.T) {
	data, err := os.ReadFile("testdata/mc-variance-swap/cases.json")
	pricingOK(t, err)
	var fixture struct {
		Cases []struct {
			Name   string           `json:"name"`
			Inputs mcVarianceInputs `json:"inputs"`
			Native mcVarianceNative `json:"native"`
		} `json:"cases"`
	}
	pricingOK(t, json.Unmarshal(data, &fixture))
	if len(fixture.Cases) < 9 {
		t.Fatal("native fixture coverage")
	}
	for _, c := range fixture.Cases {
		t.Run(c.Name, func(t *testing.T) {
			a := c.Inputs
			s := pricingMust(NewSession())
			t.Cleanup(func() { pricingOK(t, s.Close()) })
			parsed := pricingMust(time.Parse("2006-01-02", a.EvaluationDate))
			today := pricingMust(NewDate(parsed.Day(), int(parsed.Month()), parsed.Year()))
			maturity := pricingMust(today.AddDays(int64(a.MaturityDays)))
			dc := pricingMust(s.Actual365Fixed())
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(today))
			risk := pricingMust(s.NewFlatForward(today, a.Rate, dc))
			dividend := pricingMust(s.NewFlatForward(today, a.Dividend, dc))
			var vol *BlackVolTermStructure
			if a.VolatilityKind == "variance_curve" {
				dates := make([]Date, len(a.CurveDays))
				for i, days := range a.CurveDays {
					dates[i] = pricingMust(today.AddDays(int64(days)))
				}
				vol = pricingMust(s.BlackVarianceCurve(BlackVarianceCurveConfig{ReferenceDate: today, Dates: dates, Volatilities: a.CurveVols, DayCounter: dc, ForceMonotoneVariance: true}))
			} else if a.VolatilityKind == "constant" {
				vol = pricingMust(s.BlackConstantVol(BlackConstantVolConfig{ReferenceDate: today, Volatility: a.Vol, DayCounter: dc}))
			} else {
				t.Fatalf("unsupported fixture kind %q", a.VolatilityKind)
			}
			process := pricingMust(s.NewBlackScholesProcessFromCurves(a.Spot, risk, dividend, vol))
			engine := pricingMust(s.NewMCVarianceSwapEngine(process, MCVarianceSwapConfig{Steps: a.Steps, StepsPerYear: a.StepsPerYear, Samples: a.Samples, AbsoluteTolerance: a.Tolerance, MaxSamples: a.MaxSamples, Seed: a.Seed}))
			position := PositionLong
			if a.Position == "short" {
				position = PositionShort
			} else if a.Position != "long" {
				t.Fatalf("unknown position %q", a.Position)
			}
			swap := pricingMust(s.NewVarianceSwap(VarianceSwapConfig{position, a.Strike, a.Notional, today, maturity, settings}))
			pricingOK(t, swap.SetMCEngine(engine))
			variance := pricingMust(swap.Variance())
			compare := func(got, want, abs float64) { varianceSwapNear(t, got, want, math.Max(abs, 3e-12*math.Abs(want))) }
			compare(variance, c.Native.Variance, 2e-14)
			compare(pricingMust(swap.NPV()), c.Native.NPV, 2e-9)
			varianceError := pricingMust(swap.VarianceError())
			monetaryError := pricingMust(swap.ErrorEstimate())
			compare(varianceError, c.Native.VarianceError, 2e-14)
			compare(monetaryError, c.Native.ErrorEstimate, 2e-9)
			if varianceError < 0 || (position == PositionShort && monetaryError > 0) || (position == PositionLong && monetaryError < 0) {
				t.Fatal("sampling error units or native position sign")
			}
			if pricingMust(swap.Samples()) != c.Native.Samples || len(pricingMust(swap.OptionWeights())) != 0 {
				t.Fatal("sample count or MC weights")
			}
			if c.Name == "literature_curve" {
				varianceSwapNear(t, variance, .04, 3e-4)
			}
			pricingOK(t, swap.Recalculate())
			varianceSwapNear(t, pricingMust(swap.Variance()), variance, 0)
		})
	}
}
