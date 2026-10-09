package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

func TestPerformanceRatiosDecimalOracle(t *testing.T) {
	data, err := os.ReadFile("../../scripts/fixtures/performance-ratios/oracle.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Cases []struct {
			Name     string    `json:"name"`
			Returns  []float64 `json:"returns"`
			Target   float64   `json:"target"`
			Periods  float64   `json:"periods_per_year"`
			Downside float64   `json:"downside"`
			Sharpe   float64   `json:"sharpe"`
			Sortino  float64   `json:"sortino"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(data, &fixture); err != nil {
		t.Fatal(err)
	}
	for _, c := range fixture.Cases {
		t.Run(c.Name, func(t *testing.T) {
			before := append([]float64(nil), c.Returns...)
			downside, err := TargetDownsideDeviation(c.Returns, c.Target)
			if err != nil {
				t.Fatal(err)
			}
			sharpe, err := SharpeRatio(c.Returns, c.Target, c.Periods)
			if err != nil {
				t.Fatal(err)
			}
			sortino, err := SortinoRatio(c.Returns, c.Target, c.Periods)
			if err != nil {
				t.Fatal(err)
			}
			for _, pair := range [][2]float64{{downside, c.Downside}, {sharpe, c.Sharpe}, {sortino, c.Sortino}} {
				if math.Abs(pair[0]-pair[1]) > 1e-13*math.Max(1, math.Abs(pair[1])) {
					t.Fatalf("got %g want %g", pair[0], pair[1])
				}
			}
			for i := range before {
				if before[i] != c.Returns[i] {
					t.Fatal("input changed")
				}
			}
		})
	}
}

func TestPerformanceRatiosErrorsAndRecovery(t *testing.T) {
	for _, values := range [][]float64{nil, {1}, {1, math.NaN()}, {1, math.Inf(1)}} {
		if _, err := SharpeRatio(values, 0, 12); err == nil {
			t.Fatal("accepted invalid returns")
		}
		if _, err := SortinoRatio(values, 0, 12); err == nil {
			t.Fatal("accepted invalid returns")
		}
	}
	for _, target := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, err := TargetDownsideDeviation([]float64{-1, 2}, target); err == nil {
			t.Fatal("accepted target")
		}
		if _, err := SharpeRatio([]float64{-1, 2}, target, 12); err == nil {
			t.Fatal("accepted target")
		}
		if _, err := SortinoRatio([]float64{-1, 2}, target, 12); err == nil {
			t.Fatal("accepted target")
		}
	}
	for _, frequency := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		if _, err := SharpeRatio([]float64{-1, 2}, 0, frequency); err == nil {
			t.Fatal("accepted frequency")
		}
		if _, err := SortinoRatio([]float64{-1, 2}, 0, frequency); err == nil {
			t.Fatal("accepted frequency")
		}
	}
	if _, err := SharpeRatio([]float64{.01, .01}, 0, 12); err == nil {
		t.Fatal("accepted flat sample")
	}
	if _, err := SortinoRatio([]float64{.01, .02}, 0, 12); err == nil {
		t.Fatal("accepted no downside")
	}
	if value, err := TargetDownsideDeviation([]float64{.01, .02}, 0); err != nil || value != 0 {
		t.Fatalf("no downside %g %v", value, err)
	}
	for _, scale := range []float64{1e-300, 1e300} {
		got, err := SortinoRatio([]float64{-scale, scale, 2 * scale}, 0, 1)
		if err != nil || math.Abs(got-2/math.Sqrt(3)) > 1e-13 {
			t.Fatalf("scale %g: %g %v", scale, got, err)
		}
	}
	if _, err := SortinoRatio([]float64{-1, 2}, 0, 12); err != nil {
		t.Fatal(err)
	}
}
