package itofin

import (
	"errors"
	"math"
	"testing"
)

func TestGeneralStatisticsWeightedStateAndLifecycle(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	stats, err := s.NewGeneralStatistics()
	if err != nil {
		t.Fatal(err)
	}
	defer stats.Close()
	if err := stats.AddBatch([]float64{-10, -5, 1}, []float64{1, 2, 17}); err != nil {
		t.Fatal(err)
	}
	if n, err := stats.Samples(); err != nil || n != 3 {
		t.Fatalf("samples %d: %v", n, err)
	}
	if w, err := stats.WeightSum(); err != nil || w != 20 {
		t.Fatalf("weight %g: %v", w, err)
	}
	if got, err := stats.Mean(); err != nil || math.Abs(got+0.15) > 1e-12 {
		t.Fatalf("mean %g: %v", got, err)
	}
	if got, err := stats.Variance(); err != nil || math.Abs(got-12.49125) > 1e-12 {
		t.Fatalf("variance %g: %v", got, err)
	}
	if got, err := stats.ValueAtRisk(.9); err != nil || got != 5 {
		t.Fatalf("VaR %g: %v", got, err)
	}
	if got, err := stats.ExpectedShortfall(.9); err != nil || got != 10 {
		t.Fatalf("ES %g: %v", got, err)
	}
	if got, err := stats.Percentile(.5); err != nil || got != 1 {
		t.Fatalf("percentile %g: %v", got, err)
	}
	for _, check := range []struct {
		name string
		call func() (float64, error)
		want float64
	}{
		{"min", stats.Min, -10}, {"max", stats.Max, 1},
		{"standard deviation", stats.StandardDeviation, math.Sqrt(12.49125)},
		{"error estimate", stats.ErrorEstimate, math.Sqrt(12.49125 / 3)},
		{"top percentile", func() (float64, error) { return stats.TopPercentile(.1) }, 1},
		{"regret", func() (float64, error) { return stats.Regret(0) }, 100},
		{"semi variance", stats.SemiVariance, 96.045},
		{"downside variance", stats.DownsideVariance, 100},
		{"shortfall", func() (float64, error) { return stats.Shortfall(0) }, .15},
		{"average shortfall", func() (float64, error) { return stats.AverageShortfall(0) }, 20.0 / 3},
		{"potential upside", func() (float64, error) { return stats.PotentialUpside(.9) }, 1},
	} {
		got, err := check.call()
		if err != nil || math.Abs(got-check.want) > 1e-12 {
			t.Fatalf("%s: %g, %v; want %g", check.name, got, err, check.want)
		}
	}
	if err := stats.AddBatch([]float64{2, math.NaN()}, nil); err == nil {
		t.Fatal("invalid batch accepted")
	}
	if n, err := stats.Samples(); err != nil || n != 3 {
		t.Fatalf("partial batch: %d, %v", n, err)
	}
	if err := stats.Add(-20, 1); err != nil {
		t.Fatal(err)
	}
	if got, err := stats.Skewness(); err != nil || math.IsNaN(got) || math.IsInf(got, 0) {
		t.Fatalf("skewness: %g, %v", got, err)
	}
	if got, err := stats.Kurtosis(); err != nil || math.IsNaN(got) || math.IsInf(got, 0) {
		t.Fatalf("kurtosis: %g, %v", got, err)
	}
	if got, err := stats.Percentile(.01); err != nil || got != -20 {
		t.Fatalf("percentile after add %g: %v", got, err)
	}
	if err := stats.Reset(); err != nil {
		t.Fatal(err)
	}
	if n, err := stats.Samples(); err != nil || n != 0 {
		t.Fatalf("reset: %d, %v", n, err)
	}
	if _, err := stats.Mean(); err == nil {
		t.Fatal("empty mean accepted")
	}
	if err := stats.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := stats.Samples(); err == nil {
		t.Fatal("released handle accepted")
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := stats.Samples(); !errors.Is(err, ErrClosed) {
		t.Fatalf("session close error: %v", err)
	}
}

func TestGeneralStatisticsZeroWeightAndInvalidInputs(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	stats, err := s.NewGeneralStatistics()
	if err != nil {
		t.Fatal(err)
	}
	defer stats.Close()
	if err := stats.Add(1, 0); err != nil {
		t.Fatal(err)
	}
	if n, err := stats.Samples(); err != nil || n != 1 {
		t.Fatalf("zero-weight count: %d, %v", n, err)
	}
	if got, err := stats.Min(); err != nil || got != 1 {
		t.Fatalf("zero-weight min: %g, %v", got, err)
	}
	if got, err := stats.Max(); err != nil || got != 1 {
		t.Fatalf("zero-weight max: %g, %v", got, err)
	}
	if _, err := stats.Mean(); err == nil {
		t.Fatal("zero-total mean accepted")
	}
	if err := stats.AddBatch([]float64{3, 5}, nil); err != nil {
		t.Fatal(err)
	}
	if got, err := stats.Mean(); err != nil || got != 4 {
		t.Fatalf("unit-weight mean: %g, %v", got, err)
	}
	for _, probability := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, err := stats.Percentile(probability); err == nil {
			t.Fatal("nonfinite percentile accepted")
		}
		if _, err := stats.TopPercentile(probability); err == nil {
			t.Fatal("nonfinite top percentile accepted")
		}
	}
	for _, input := range []struct{ value, weight float64 }{{math.NaN(), 1}, {1, -1}, {1, math.Inf(1)}} {
		if err := stats.Add(input.value, input.weight); err == nil {
			t.Fatalf("accepted %+v", input)
		}
	}
	if n, err := stats.Samples(); err != nil || n != 3 {
		t.Fatalf("invalid add changed state: %d, %v", n, err)
	}
	if err := stats.Reset(); err != nil {
		t.Fatal(err)
	}
	if err := stats.AddBatch([]float64{-2, -1, 1}, []float64{0, 0, 1}); err != nil {
		t.Fatal(err)
	}
	for _, query := range []func() (float64, error){
		stats.SemiVariance, stats.SemiDeviation,
		stats.DownsideVariance, stats.DownsideDeviation,
		func() (float64, error) { return stats.Regret(0) },
		func() (float64, error) { return stats.AverageShortfall(0) },
	} {
		if _, err := query(); err == nil {
			t.Fatal("zero-positive-weight conditional tail accepted")
		}
	}
}
