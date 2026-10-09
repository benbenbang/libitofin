package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

func TestMaximumDrawdownIndependentFixturesAndSnapshots(t *testing.T) {
	data, err := os.ReadFile("testdata/drawdown.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Cases []struct {
			Name     string
			Values   []float64
			Expected DrawdownResult
		}
	}
	if err := json.Unmarshal(data, &fixture); err != nil {
		t.Fatal(err)
	}
	if len(fixture.Cases) != 8 {
		t.Fatal("missing drawdown fixtures")
	}
	for _, c := range fixture.Cases {
		t.Run(c.Name, func(t *testing.T) {
			result, err := MaximumDrawdown(c.Values)
			if err != nil || result != c.Expected {
				t.Fatalf("got %+v, %v; want %+v", result, err, c.Expected)
			}
			c.Values[0] = -99
			if result != c.Expected {
				t.Fatal("result changed after input mutation")
			}
		})
	}
}

func TestMaximumDrawdownErrorsAndFiniteExtremes(t *testing.T) {
	for _, values := range [][]float64{nil, {}, {0}, {-1}, {math.NaN()}, {math.Inf(1)}, {math.Inf(-1)}, {100, 1, 0}, {100, 1, math.NaN()}} {
		result, err := MaximumDrawdown(values)
		if err == nil || result != (DrawdownResult{}) {
			t.Fatalf("expected zero result and error for %v: %+v, %v", values, result, err)
		}
	}
	for _, c := range []struct {
		Values []float64
		Loss   float64
	}{
		{[]float64{math.MaxFloat64, math.MaxFloat64 / 2}, 0.5},
		{[]float64{math.MaxFloat64, math.SmallestNonzeroFloat64}, 1},
		{[]float64{2 * math.SmallestNonzeroFloat64, math.SmallestNonzeroFloat64}, 0.5},
		{[]float64{1, math.Nextafter(1, 0)}, 1 - math.Nextafter(1, 0)},
	} {
		result, err := MaximumDrawdown(c.Values)
		if err != nil || result.Drawdown != c.Loss || result.PeakIndex != 0 || result.TroughIndex != 1 {
			t.Fatalf("got %+v, %v", result, err)
		}
	}
	for i := 0; i < 100; i++ {
		result, err := MaximumDrawdown([]float64{100, 80})
		if err != nil || result.Drawdown != 0.2 {
			t.Fatalf("got %+v, %v", result, err)
		}
	}
}
