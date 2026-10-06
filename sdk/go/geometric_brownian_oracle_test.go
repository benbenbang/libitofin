package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

func TestGeometricBrownianQuantLibOracle(t *testing.T) {
	var fixture struct {
		Cases []struct {
			Name   string `json:"name"`
			Inputs struct {
				Initial    float64 `json:"initial_value"`
				Mu         float64 `json:"mu"`
				Volatility float64 `json:"volatility"`
				Time       float64 `json:"time"`
				State      float64 `json:"state"`
				DT         float64 `json:"dt"`
				DW         float64 `json:"dw"`
			} `json:"inputs"`
			Native map[string]float64 `json:"native"`
		} `json:"cases"`
	}
	data, err := os.ReadFile("testdata/geometric-brownian/cases.json")
	pricingOK(t, err)
	pricingOK(t, json.Unmarshal(data, &fixture))
	var meta struct {
		Tolerances struct {
			Relative       float64 `json:"relative"`
			ScalarAbsolute float64 `json:"scalar_absolute"`
		} `json:"tolerances"`
	}
	data, err = os.ReadFile("testdata/geometric-brownian/oracle.json")
	pricingOK(t, err)
	pricingOK(t, json.Unmarshal(data, &meta))
	if len(fixture.Cases) != 9 || meta.Tolerances.Relative != 3e-12 || meta.Tolerances.ScalarAbsolute != 2e-14 {
		t.Fatal("missing native cases or changed canonical tolerance")
	}
	for _, c := range fixture.Cases {
		t.Run(c.Name, func(t *testing.T) {
			s := pricingMust(NewSession())
			t.Cleanup(func() { pricingOK(t, s.Close()) })
			a := c.Inputs
			p := pricingMust(s.NewGeometricBrownianMotionProcess(a.Initial, a.Mu, a.Volatility))
			got := map[string]float64{
				"x0": pricingMust(p.X0()), "drift": pricingMust(p.Drift(a.Time, a.State)),
				"diffusion":     pricingMust(p.Diffusion(a.Time, a.State)),
				"expectation":   pricingMust(p.Expectation(a.Time, a.State, a.DT)),
				"variance":      pricingMust(p.Variance(a.Time, a.State, a.DT)),
				"std_deviation": pricingMust(p.StdDeviation(a.Time, a.State, a.DT)),
				"evolve":        pricingMust(p.Evolve(a.Time, a.State, a.DT, a.DW)),
			}
			if len(c.Native) != len(got) {
				t.Fatalf("native outputs: got %d want %d", len(c.Native), len(got))
			}
			for name, value := range got {
				want, ok := c.Native[name]
				if !ok || math.IsInf(value, 0) || math.IsNaN(value) || math.IsInf(want, 0) || math.IsNaN(want) {
					t.Fatalf("missing/nonfinite output %s: %g", name, value)
				}
				limit := math.Max(meta.Tolerances.ScalarAbsolute, meta.Tolerances.Relative*math.Abs(want))
				if math.Abs(value-want) > limit {
					t.Fatalf("%s: got %.17g want %.17g tolerance %g", name, value, want, limit)
				}
			}
			pricingNear(t, pricingMust(p.Mu()), a.Mu, 0)
			pricingNear(t, pricingMust(p.Volatility()), a.Volatility, 0)
		})
	}
}
