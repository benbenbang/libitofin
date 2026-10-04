package itofin

import (
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"testing"
)

func convergenceFixture(t *testing.T, path string, destination any) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, destination); err != nil {
		t.Fatal(err)
	}
}

func TestStatisticsConvergenceIndependentOracle(t *testing.T) {
	directory := filepath.Join("testdata", "convergence-statistics")
	var manifest struct {
		Cases []string `json:"cases"`
	}
	convergenceFixture(t, filepath.Join(directory, "oracle.json"), &manifest)
	if len(manifest.Cases) == 0 {
		t.Fatal("empty native convergence oracle")
	}
	for _, file := range manifest.Cases {
		t.Run(file, func(t *testing.T) {
			var fixture struct {
				Observations []float64 `json:"observations"`
				Weights      []float64 `json:"weights"`
				Native       struct {
					Samples   int                `json:"samples"`
					WeightSum float64            `json:"weight_sum"`
					Mean      *float64           `json:"mean"`
					Table     []ConvergencePoint `json:"table"`
				} `json:"native"`
				Exact struct {
					Table []ConvergencePoint `json:"table"`
				} `json:"exact"`
			}
			convergenceFixture(t, filepath.Join(directory, file), &fixture)
			batch, err := StatisticsConvergence(fixture.Observations, fixture.Weights)
			if err != nil {
				t.Fatal(err)
			}
			s, err := NewSession()
			if err != nil {
				t.Fatal(err)
			}
			defer s.Close()
			stats, err := s.NewConvergenceStatistics()
			if err != nil {
				t.Fatal(err)
			}
			defer stats.Close()
			if err := stats.AddBatch(fixture.Observations, fixture.Weights); err != nil {
				t.Fatal(err)
			}
			table, err := stats.Table()
			if err != nil {
				t.Fatal(err)
			}
			for _, got := range [][]ConvergencePoint{batch, table} {
				if len(got) != len(fixture.Native.Table) || len(got) != len(fixture.Exact.Table) {
					t.Fatalf("length mismatch %v", got)
				}
				for i, point := range got {
					for _, expected := range []ConvergencePoint{fixture.Native.Table[i], fixture.Exact.Table[i]} {
						if point.Samples != expected.Samples || math.IsNaN(point.Mean) || math.IsInf(point.Mean, 0) || math.Abs(point.Mean-expected.Mean) > 2e-12*math.Max(1, math.Abs(expected.Mean)) {
							t.Fatalf("checkpoint %v != %v", point, expected)
						}
					}
				}
			}
			if count, err := stats.Samples(); err != nil || count != fixture.Native.Samples {
				t.Fatalf("count %d %v", count, err)
			}
			if weight, err := stats.WeightSum(); err != nil || math.IsNaN(weight) || math.IsInf(weight, 0) || math.Abs(weight-fixture.Native.WeightSum) > 2e-12*math.Max(1, math.Abs(fixture.Native.WeightSum)) {
				t.Fatalf("weight %g %v", weight, err)
			}
			mean, err := stats.Mean()
			if fixture.Native.Mean == nil {
				if err == nil {
					t.Fatal("empty mean accepted")
				}
			} else if err != nil || math.IsNaN(mean) || math.IsInf(mean, 0) || math.Abs(mean-*fixture.Native.Mean) > 2e-12*math.Max(1, math.Abs(*fixture.Native.Mean)) {
				t.Fatalf("mean %g %v", mean, err)
			}

		})
	}
}
