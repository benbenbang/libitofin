package itofin

import (
	"math"
	"reflect"
	"testing"
)

func statisticsClose(t *testing.T, name string, want, got float64, err error) {
	t.Helper()
	if err != nil {
		t.Fatalf("%s: %v", name, err)
	}
	if math.IsNaN(got) || math.Abs(got-want) > 1e-14 {
		t.Fatalf("%s: got %.17g, want %.17g", name, got, want)
	}
}

func TestStatisticsWeightedHandCases(t *testing.T) {
	values := []float64{3, 1, 2}
	weights := []float64{1, 1, 2}
	got, err := StatisticsMean(values, weights)
	statisticsClose(t, "mean", 2, got, err)
	got, err = StatisticsSampleVariance(values, weights)
	statisticsClose(t, "sample variance", 0.75, got, err)
	got, err = StatisticsStandardDeviation(values, weights)
	statisticsClose(t, "standard deviation", math.Sqrt(0.75), got, err)
	got, err = StatisticsPercentile(values, weights, 0.5)
	statisticsClose(t, "median", 2, got, err)
	got, err = StatisticsPercentile(values, weights, 1)
	statisticsClose(t, "maximum", 3, got, err)
	if !reflect.DeepEqual(values, []float64{3, 1, 2}) || !reflect.DeepEqual(weights, []float64{1, 1, 2}) {
		t.Fatalf("statistics changed inputs: %v, %v", values, weights)
	}
	got, err = StatisticsSampleVariance([]float64{1, 2, 3}, []float64{1, 0, 1})
	statisticsClose(t, "zero weight contributes to count", 1.5, got, err)
}

func TestStatisticsEmpiricalRisk(t *testing.T) {
	values := []float64{2, -5, 1, -10}
	weights := []float64{14, 2, 3, 1}
	got, err := StatisticsValueAtRisk(values, weights, 0.9)
	statisticsClose(t, "weighted VaR", 5, got, err)
	got, err = StatisticsExpectedShortfall(values, weights, 0.9)
	statisticsClose(t, "weighted ES", 10, got, err)
	parityValues := []float64{-5, -2, 1, 2}
	parityWeights := []float64{1, 10, 1, 8}
	got, err = StatisticsValueAtRisk(parityValues, parityWeights, 0.9)
	statisticsClose(t, "shared VaR", 2, got, err)
	got, err = StatisticsExpectedShortfall(parityValues, parityWeights, 0.9)
	statisticsClose(t, "shared ES", 5, got, err)
	got, err = StatisticsValueAtRisk([]float64{1, 2}, nil, 0.9)
	statisticsClose(t, "positive observations have no loss", 0, got, err)
	if _, err := StatisticsExpectedShortfall([]float64{-10, 0}, nil, 0.9); err == nil {
		t.Fatal("accepted an empty strict expected-shortfall tail")
	}
	if _, err := StatisticsExpectedShortfall([]float64{-10, -5, 0}, []float64{0, 1, 9}, 0.9); err == nil {
		t.Fatal("accepted a zero-weight strict expected-shortfall tail")
	}
}

func TestStatisticsUnitWeightsMatchExplicitWeights(t *testing.T) {
	values := []float64{-10, -5, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18}
	weights := make([]float64, len(values))
	for i := range weights {
		weights[i] = 1
	}
	operations := []struct {
		name string
		call func([]float64, []float64) (float64, error)
	}{
		{"mean", StatisticsMean},
		{"sample variance", StatisticsSampleVariance},
		{"standard deviation", StatisticsStandardDeviation},
		{"percentile", func(x, w []float64) (float64, error) { return StatisticsPercentile(x, w, 0.5) }},
		{"VaR", func(x, w []float64) (float64, error) { return StatisticsValueAtRisk(x, w, 0.9) }},
		{"ES", func(x, w []float64) (float64, error) { return StatisticsExpectedShortfall(x, w, 0.9) }},
	}
	for _, operation := range operations {
		unit, err := operation.call(values, nil)
		if err != nil {
			t.Fatalf("%s with unit weights: %v", operation.name, err)
		}
		explicit, err := operation.call(values, weights)
		statisticsClose(t, operation.name, unit, explicit, err)
	}
}

func TestStatisticsRejectsInvalidInputs(t *testing.T) {
	for _, values := range [][]float64{nil, {math.NaN()}, {math.Inf(1)}, {math.Inf(-1)}} {
		if _, err := StatisticsMean(values, nil); err == nil {
			t.Fatalf("accepted invalid observations: %v", values)
		}
	}
	for _, weights := range [][]float64{{}, {1}, {-1, 2}, {math.NaN(), 1}, {math.Inf(1), 1}, {0, 0}, {math.MaxFloat64, math.MaxFloat64}} {
		if _, err := StatisticsMean([]float64{1, 2}, weights); err == nil {
			t.Fatalf("accepted invalid weights: %v", weights)
		}
	}
	if _, err := StatisticsSampleVariance([]float64{1}, nil); err == nil {
		t.Fatal("accepted one observation for sample variance")
	}
	if _, err := StatisticsStandardDeviation([]float64{1}, nil); err == nil {
		t.Fatal("accepted one observation for standard deviation")
	}
	for _, probability := range []float64{-0.1, 0, 1.01, math.NaN(), math.Inf(1)} {
		if _, err := StatisticsPercentile([]float64{1}, nil, probability); err == nil {
			t.Fatalf("accepted invalid percentile: %v", probability)
		}
	}
	for _, probability := range []float64{0.899, 1, math.NaN(), math.Inf(1)} {
		if _, err := StatisticsValueAtRisk([]float64{-1}, nil, probability); err == nil {
			t.Fatalf("accepted invalid VaR confidence: %v", probability)
		}
		if _, err := StatisticsExpectedShortfall([]float64{-1}, nil, probability); err == nil {
			t.Fatalf("accepted invalid ES confidence: %v", probability)
		}
	}
}
