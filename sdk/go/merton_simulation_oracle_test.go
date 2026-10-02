package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

type mertonPathOracleInput struct {
	Spot              float64 `json:"spot"`
	Drift             float64 `json:"drift"`
	Volatility        float64 `json:"volatility"`
	JumpIntensity     float64 `json:"jump_intensity"`
	LogMeanJump       float64 `json:"log_mean_jump"`
	LogJumpVolatility float64 `json:"log_jump_volatility"`
	Horizon           float64 `json:"horizon"`
	Steps             int     `json:"steps"`
	Paths             int     `json:"paths"`
	Seed              uint32  `json:"seed"`
}

func mertonPathOracleNear(t *testing.T, actual, expected []float64) {
	t.Helper()
	if len(actual) != len(expected) {
		t.Fatalf("oracle shape mismatch: %d vs %d", len(actual), len(expected))
	}
	for i, want := range expected {
		tolerance := 2e-14 * math.Max(1, math.Abs(want))
		if math.IsNaN(actual[i]) || math.IsInf(actual[i], 0) || math.Abs(actual[i]-want) > tolerance {
			t.Fatalf("independent oracle value %d: %.17g vs %.17g, tolerance %g", i, actual[i], want, tolerance)
		}
	}
}

func TestMertonSimulationIndependentOracles(t *testing.T) {
	for _, path := range []string{"testdata/merton-paths-oracle.json", "testdata/merton-paths-additional.json"} {
		data := pricingMust(os.ReadFile(path))
		var fixture struct {
			Cases []struct {
				Name     string                `json:"name"`
				Input    mertonPathOracleInput `json:"input"`
				Full     []float64             `json:"expected_full"`
				Terminal []float64             `json:"expected_terminal"`
			} `json:"cases"`
		}
		pricingOK(t, json.Unmarshal(data, &fixture))
		if len(fixture.Cases) != 6 {
			t.Fatalf("expected six independent cases in %s, got %d", path, len(fixture.Cases))
		}
		for _, test := range fixture.Cases {
			t.Run(test.Name, func(t *testing.T) {
				i := test.Input
				c := MertonConfig{
					Spot: i.Spot, Drift: i.Drift, Volatility: i.Volatility, JumpIntensity: i.JumpIntensity,
					LogMeanJump: i.LogMeanJump, LogJumpVolatility: i.LogJumpVolatility, Horizon: i.Horizon,
					Steps: i.Steps, Paths: i.Paths, Seed: i.Seed,
				}
				full := pricingMust(SimulateMerton(c))
				mertonPathOracleNear(t, full.Values, test.Full)
				c.TerminalOnly = true
				terminal := pricingMust(SimulateMerton(c))
				mertonPathOracleNear(t, terminal.Values, test.Terminal)
				for path, value := range terminal.Values {
					if math.Float64bits(value) != math.Float64bits(full.Values[path*(c.Steps+1)+c.Steps]) {
						t.Fatal("terminal/full bits differ")
					}
				}
			})
		}
	}
}
