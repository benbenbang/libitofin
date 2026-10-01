package itofin

import (
	"errors"
	"math"
	"testing"
)

func TestIncrementalStatisticsWeightedState(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	stats, err := s.NewIncrementalStatistics()
	if err != nil {
		t.Fatal(err)
	}
	defer stats.Close()
	if err := stats.AddBatch([]float64{-4, -2, 2, 8}, []float64{1, 2, 1, 0}); err != nil {
		t.Fatal(err)
	}
	count, err := stats.Samples()
	if err != nil || count != 4 {
		t.Fatalf("count: %d, %v", count, err)
	}
	downside, err := stats.DownsideSamples()
	if err != nil || downside != 2 {
		t.Fatalf("downside count: %d, %v", downside, err)
	}
	for _, row := range []struct {
		name string
		fn   func() (float64, error)
		want float64
	}{
		{"weight", stats.WeightSum, 4}, {"downside weight", stats.DownsideWeightSum, 3},
		{"min", stats.Min, -4}, {"max", stats.Max, 8},
		{"mean", stats.Mean, -1.5}, {"variance", stats.Variance, 19.0 / 3.0},
		{"stddev", stats.StandardDeviation, math.Sqrt(19.0 / 3.0)},
		{"downside variance", stats.DownsideVariance, 16}, {"downside deviation", stats.DownsideDeviation, 4},
	} {
		got, err := row.fn()
		if err != nil || math.Abs(got-row.want) > 1e-12 {
			t.Fatalf("%s: %g, %v", row.name, got, err)
		}
	}
	if err := stats.AddBatch([]float64{1, math.NaN()}, nil); err == nil {
		t.Fatal("invalid batch accepted")
	}
	count, err = stats.Samples()
	if err != nil || count != 4 {
		t.Fatalf("invalid batch mutated state: %d, %v", count, err)
	}
	if err := stats.Reset(); err != nil {
		t.Fatal(err)
	}
	count, err = stats.Samples()
	if err != nil || count != 0 {
		t.Fatalf("reset count: %d, %v", count, err)
	}
	if _, err := stats.Mean(); err == nil {
		t.Fatal("empty mean accepted")
	}
	if err := stats.Add(1e100, 1); err == nil {
		t.Fatal("overflowing first moment state accepted")
	}
	if err := stats.AddBatch([]float64{1, 1e100}, nil); err == nil {
		t.Fatal("overflowing batch state accepted")
	}
	count, err = stats.Samples()
	if err != nil || count != 0 {
		t.Fatalf("overflowing update mutated state: %d, %v", count, err)
	}
	if err := stats.AddBatch([]float64{1, 2, 3, 4}, nil); err != nil {
		t.Fatal(err)
	}
	skew, err := stats.Skewness()
	if err != nil || math.Abs(skew) > 1e-12 {
		t.Fatalf("skewness: %g, %v", skew, err)
	}
	kurtosis, err := stats.Kurtosis()
	if err != nil || math.Abs(kurtosis+1.2) > 1e-12 {
		t.Fatalf("kurtosis: %g, %v", kurtosis, err)
	}
	if err := stats.Close(); err != nil {
		t.Fatal(err)
	}
	if err := stats.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := stats.Samples(); err == nil {
		t.Fatal("released handle accepted")
	}
}

func TestIncrementalStatisticsStableMomentAndSessionClose(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	stats, err := s.NewIncrementalStatistics()
	if err != nil {
		t.Fatal(err)
	}
	if err := stats.Add(-20, 0); err != nil {
		t.Fatal(err)
	}
	if _, err := stats.Mean(); err == nil {
		t.Fatal("zero total weight accepted")
	}
	for offset := 0; offset < 4; offset++ {
		if err := stats.Add(1e12+float64(offset), 1); err != nil {
			t.Fatal(err)
		}
	}
	got, err := stats.Mean()
	if err != nil || got != 1e12+1.5 {
		t.Fatalf("mean: %g, %v", got, err)
	}
	got, err = stats.Variance()
	if err != nil || math.Abs(got-1.5625) > 1e-10 {
		t.Fatalf("variance: %g, %v", got, err)
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := stats.Samples(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
	if err := stats.Close(); err != nil {
		t.Fatalf("close after session: %v", err)
	}
}
