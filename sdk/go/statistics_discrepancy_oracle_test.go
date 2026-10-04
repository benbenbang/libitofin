package itofin

import (
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"testing"
)

func discrepancyFixture(t *testing.T, path string, destination any) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, destination); err != nil {
		t.Fatal(err)
	}
}

func TestStatisticsDiscrepancyIndependentOracle(t *testing.T) {
	directory := filepath.Join("testdata", "discrepancy-statistics")
	var manifest struct {
		Cases []string `json:"cases"`
	}
	discrepancyFixture(t, filepath.Join(directory, "oracle.json"), &manifest)
	if len(manifest.Cases) == 0 {
		t.Fatal("empty native discrepancy oracle")
	}
	for _, file := range manifest.Cases {
		t.Run(file, func(t *testing.T) {
			var fixture struct {
				Samples [][]float64 `json:"samples"`
				Weights []float64   `json:"weights"`
				Native  float64     `json:"native_discrepancy"`
				Exact   float64     `json:"exact_discrepancy"`
			}
			discrepancyFixture(t, filepath.Join(directory, file), &fixture)
			got, err := StatisticsDiscrepancy(fixture.Samples, fixture.Weights)
			if err != nil {
				t.Fatal(err)
			}
			for _, want := range []float64{fixture.Native, fixture.Exact} {
				if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > 2e-12*math.Max(1, math.Abs(want)) {
					t.Fatalf("discrepancy %g != %g", got, want)
				}
			}
		})
	}
}
