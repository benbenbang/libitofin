package itofin

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"testing"
)

type varianceSwapOracleInput struct {
	EvaluationDate string    `json:"evaluation_date"`
	StartDate      string    `json:"start_date"`
	MaturityDate   string    `json:"maturity_date"`
	DayCounter     string    `json:"day_counter"`
	Spot           float64   `json:"spot"`
	Rate           float64   `json:"risk_free_rate"`
	Dividend       float64   `json:"dividend_yield"`
	Volatility     float64   `json:"volatility"`
	Strike         float64   `json:"variance_strike"`
	Notional       float64   `json:"notional"`
	Position       string    `json:"position"`
	Dk             float64   `json:"dk"`
	Calls          []float64 `json:"call_strikes"`
	Puts           []float64 `json:"put_strikes"`
	SmileStrikes   []float64 `json:"smile_strikes"`
	SmileVols      []float64 `json:"smile_vols"`
}
type varianceSwapOracleResult struct {
	Variance float64 `json:"variance"`
	NPV      float64 `json:"npv"`
	Discount float64 `json:"discount_factor"`
	Weights  []struct {
		Kind   string  `json:"option_type"`
		Strike float64 `json:"strike"`
		Weight float64 `json:"weight"`
	} `json:"weights"`
}
type varianceSwapOracleCase struct {
	Name    string                   `json:"name"`
	Inputs  varianceSwapOracleInput  `json:"inputs"`
	Native  varianceSwapOracleResult `json:"native"`
	Updates []struct {
		Name   string `json:"name"`
		Market struct {
			Spot       float64 `json:"spot"`
			Rate       float64 `json:"risk_free_rate"`
			Dividend   float64 `json:"dividend_yield"`
			Volatility float64 `json:"volatility"`
		} `json:"market"`
		CachedVariance float64                  `json:"cached_variance"`
		CachedNPV      float64                  `json:"cached_npv"`
		Recalculated   varianceSwapOracleResult `json:"recalculated"`
	} `json:"updates"`
}

func varianceSwapOracleDate(t *testing.T, iso string) Date {
	t.Helper()
	var y, m, d int32
	if n, e := fmt.Sscanf(iso, "%d-%d-%d", &y, &m, &d); e != nil || n != 3 {
		t.Fatalf("invalid oracle date %q: %v", iso, e)
	}
	return pricingMust(NewDate(int(d), int(m), int(y)))
}
func varianceSwapOracleMarket(t *testing.T, in varianceSwapOracleInput) *varianceSwapMarket {
	t.Helper()
	m := &varianceSwapMarket{s: pricingMust(NewSession())}
	t.Cleanup(func() { pricingOK(t, m.s.Close()) })
	m.today = varianceSwapOracleDate(t, in.EvaluationDate)
	m.maturity = varianceSwapOracleDate(t, in.MaturityDate)
	if in.DayCounter != "Actual365Fixed" {
		t.Fatalf("unsupported oracle day counter %q", in.DayCounter)
	}
	dc := pricingMust(m.s.Actual365Fixed())
	m.settings = pricingMust(m.s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	for i, v := range []float64{in.Rate, in.Dividend, in.Volatility} {
		m.quotes[i] = pricingMust(m.s.NewSimpleQuote(v))
	}
	m.risk = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[0], dc))
	m.dividend = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[1], dc))
	if len(in.SmileStrikes) > 0 {
		vols := make([][]float64, len(in.SmileVols))
		for i, v := range in.SmileVols {
			vols[i] = []float64{v}
		}
		m.vol = pricingMust(m.s.BlackVarianceSurface(BlackVarianceSurfaceConfig{ReferenceDate: m.today, Dates: []Date{m.maturity}, Strikes: in.SmileStrikes, Volatilities: vols, DayCounter: dc}))
	} else {
		m.vol = pricingMust(m.s.NewBlackConstantVolFromQuote(m.today, m.quotes[2], dc, nil))
	}
	m.process = pricingMust(m.s.NewBlackScholesProcessFromCurves(in.Spot, m.risk, m.dividend, m.vol))
	m.engine = pricingMust(m.s.NewReplicatingVarianceSwapEngine(ReplicatingVarianceSwapEngineConfig{m.process, in.Dk, in.Calls, in.Puts}))
	position := PositionLong
	if in.Position == "short" {
		position = PositionShort
	} else if in.Position != "long" {
		t.Fatal("invalid oracle position")
	}
	m.swap = pricingMust(m.s.NewVarianceSwap(VarianceSwapConfig{position, in.Strike, in.Notional, varianceSwapOracleDate(t, in.StartDate), m.maturity, m.settings}))
	pricingOK(t, m.swap.SetEngine(m.engine))
	return m
}
func TestVarianceSwapNativeQuantLibOracle(t *testing.T) {
	dir := "testdata/variance-swap"
	var manifest struct {
		Cases []string `json:"cases"`
	}
	pricingOK(t, json.Unmarshal(pricingMust(os.ReadFile(filepath.Join(dir, "oracle.json"))), &manifest))
	if len(manifest.Cases) != 15 {
		t.Fatalf("expected 15 native cases, got %d", len(manifest.Cases))
	}
	for _, file := range manifest.Cases {
		var fixture varianceSwapOracleCase
		pricingOK(t, json.Unmarshal(pricingMust(os.ReadFile(filepath.Join(dir, file))), &fixture))
		t.Run(fixture.Name, func(t *testing.T) {
			m := varianceSwapOracleMarket(t, fixture.Inputs)
			varianceSwapNear(t, pricingMust(m.swap.Variance()), fixture.Native.Variance, math.Max(2e-14, 3e-12*math.Abs(fixture.Native.Variance)))
			varianceSwapNear(t, pricingMust(m.swap.NPV()), fixture.Native.NPV, math.Max(2e-9, 3e-12*math.Abs(fixture.Native.NPV)))
			weights := pricingMust(m.swap.OptionWeights())
			if len(weights) != len(fixture.Native.Weights) {
				t.Fatal("native weight count")
			}
			for i, w := range weights {
				n := fixture.Native.Weights[i]
				kind := Call
				if n.Kind == "put" {
					kind = Put
				} else if n.Kind != "call" {
					t.Fatal("invalid native weight type")
				}
				if w.OptionType != kind {
					t.Fatal("native weight type")
				}
				varianceSwapNear(t, w.Strike, n.Strike, 0)
				varianceSwapNear(t, w.Weight, n.Weight, math.Max(2e-14, 3e-12*math.Abs(n.Weight)))
			}
			for _, u := range fixture.Updates {
				t.Run(u.Name, func(t *testing.T) {
					if u.Market.Spot != fixture.Inputs.Spot {
						inputs := fixture.Inputs
						inputs.Spot, inputs.Rate = u.Market.Spot, u.Market.Rate
						inputs.Dividend, inputs.Volatility = u.Market.Dividend, u.Market.Volatility
						rebuilt := varianceSwapOracleMarket(t, inputs)
						varianceSwapNear(t, pricingMust(rebuilt.swap.Variance()), u.Recalculated.Variance, math.Max(2e-14, 3e-12*math.Abs(u.Recalculated.Variance)))
						varianceSwapNear(t, pricingMust(rebuilt.swap.NPV()), u.Recalculated.NPV, math.Max(2e-9, 3e-12*math.Abs(u.Recalculated.NPV)))
						pricingOK(t, rebuilt.swap.Recalculate())
						varianceSwapNear(t, pricingMust(rebuilt.swap.Variance()), u.Recalculated.Variance, math.Max(2e-14, 3e-12*math.Abs(u.Recalculated.Variance)))
						return
					}
					for i, v := range []float64{u.Market.Rate, u.Market.Dividend, u.Market.Volatility} {
						pricingOK(t, m.quotes[i].SetValue(v))
					}
					if pricingMust(m.swap.IsCalculated()) {
						t.Fatal("update did not invalidate")
					}
					varianceSwapNear(t, pricingMust(m.swap.Variance()), u.Recalculated.Variance, math.Max(2e-14, 3e-12*math.Abs(u.Recalculated.Variance)))
					varianceSwapNear(t, pricingMust(m.swap.NPV()), u.Recalculated.NPV, math.Max(2e-9, 3e-12*math.Abs(u.Recalculated.NPV)))
					pricingOK(t, m.swap.Recalculate())
					varianceSwapNear(t, pricingMust(m.swap.Variance()), u.Recalculated.Variance, math.Max(2e-14, 3e-12*math.Abs(u.Recalculated.Variance)))
				})
			}
		})
	}
}
