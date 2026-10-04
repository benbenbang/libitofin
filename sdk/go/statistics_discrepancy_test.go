package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestStatisticsDiscrepancyUnitCubeAndWeights(t *testing.T) {
	samples := [][]float64{{.5, .5}}
	before := [][]float64{{.5, .5}}
	unit, err := StatisticsDiscrepancy(samples, nil)
	want := math.Sqrt(23.0 / 288)
	if err != nil || math.IsNaN(unit) || math.IsInf(unit, 0) || math.Abs(unit-want) > 1e-14 {
		t.Fatalf("got %g %v want %g", unit, err, want)
	}
	explicit, err := StatisticsDiscrepancy(samples, []float64{1})
	if err != nil || explicit != unit {
		t.Fatalf("unit weights %g %v", explicit, err)
	}
	if !reflect.DeepEqual(samples, before) {
		t.Fatal("input mutated")
	}
	for _, samples := range [][][]float64{{{0, 0}, {1, 1}}, {{.25, .75}, {.25, .75}}, {{.5, .5, .5}}, {make([]float64, 256)}} {
		result, err := StatisticsDiscrepancy(samples, nil)
		if err != nil || math.IsNaN(result) || math.IsInf(result, 0) || result < 0 {
			t.Fatalf("valid points %g %v", result, err)
		}
	}
}

func TestStatisticsDiscrepancyRejectsInvalidInputsAndBounds(t *testing.T) {
	for _, samples := range [][][]float64{
		nil, {{}}, {{.5}}, {{.5, .5}, {.5}}, {{-.1, .5}}, {{1.1, .5}},
		{{math.NaN(), .5}}, {{math.Inf(1), .5}}, {make([]float64, 257)},
		make([][]float64, 4097),
	} {
		if _, err := StatisticsDiscrepancy(samples, nil); err == nil {
			t.Fatalf("invalid samples accepted %v", samples)
		}
	}
	for _, weights := range [][]float64{{}, {0}, {2}, {-1}, {math.NaN()}, {math.Inf(1)}, {1, 1}} {
		if _, err := StatisticsDiscrepancy([][]float64{{.5, .5}}, weights); err == nil {
			t.Fatalf("invalid weights accepted %v", weights)
		}
	}
	for _, shape := range [][2]int{{4096, 6}, {4096, 256}} {
		samples := make([][]float64, shape[0])
		samples[0] = make([]float64, shape[1])
		if _, err := StatisticsDiscrepancy(samples, nil); err == nil {
			t.Fatalf("excessive shape accepted %v", shape)
		}
	}
}
