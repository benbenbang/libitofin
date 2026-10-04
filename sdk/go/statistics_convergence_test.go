package itofin

import (
	"errors"
	"math"
	"reflect"
	"sync"
	"testing"
)

func TestStatisticsConvergenceCheckpoints(t *testing.T) {
	for _, size := range []int{0, 1, 2, 3, 6, 7, 14, 15, 16} {
		values := make([]float64, size)
		for i := range values {
			values[i] = float64(i + 1)
		}
		got, err := StatisticsConvergence(values, nil)
		if err != nil {
			t.Fatal(err)
		}
		var want []ConvergencePoint
		for count := 1; count <= size; count = 2*count + 1 {
			want = append(want, ConvergencePoint{count, float64(count+1) / 2})
		}
		if len(got) != len(want) {
			t.Fatalf("count %d: %v", size, got)
		}
		for i := range want {
			if got[i] != want[i] {
				t.Fatalf("count %d: %v != %v", size, got, want)
			}
		}
	}
	values, weights := []float64{2, 4, 8, 10}, []float64{1, 2, 1, 0}
	got, err := StatisticsConvergence(values, weights)
	if err != nil || !reflect.DeepEqual(got, []ConvergencePoint{{1, 2}, {3, 4.5}}) {
		t.Fatalf("%v %v", got, err)
	}
	got[0].Mean = 99
	again, err := StatisticsConvergence(values, weights)
	if err != nil || again[0].Mean != 2 || values[0] != 2 || weights[0] != 1 {
		t.Fatalf("aliased result %v %v", again, err)
	}
}

func TestConvergenceStatisticsStateAndReset(t *testing.T) {
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
	if table, err := stats.Table(); err != nil || len(table) != 0 {
		t.Fatalf("empty table %v %v", table, err)
	}
	if _, err := stats.Mean(); err == nil {
		t.Fatal("empty mean accepted")
	}
	if err := stats.Add(2, 0); err == nil {
		t.Fatal("zero checkpoint accepted")
	}
	if count, err := stats.Samples(); err != nil || count != 0 {
		t.Fatalf("zero changed count %d %v", count, err)
	}
	if err := stats.Add(2, 1); err != nil {
		t.Fatal(err)
	}
	if err := stats.AddBatch([]float64{4, 8, 10}, []float64{2, 1, 0}); err != nil {
		t.Fatal(err)
	}
	if count, err := stats.Samples(); err != nil || count != 4 {
		t.Fatalf("count %d %v", count, err)
	}
	if weight, err := stats.WeightSum(); err != nil || weight != 4 {
		t.Fatalf("weight %g %v", weight, err)
	}
	if mean, err := stats.Mean(); err != nil || mean != 4.5 {
		t.Fatalf("mean %g %v", mean, err)
	}
	table, err := stats.Table()
	if err != nil || !reflect.DeepEqual(table, []ConvergencePoint{{1, 2}, {3, 4.5}}) {
		t.Fatalf("table %v %v", table, err)
	}
	table[0].Mean = 99
	fresh, err := stats.Table()
	if err != nil || fresh[0].Mean != 2 {
		t.Fatalf("aliased table %v %v", fresh, err)
	}
	if err := stats.Add(14, 1); err != nil {
		t.Fatal(err)
	}
	if mean, err := stats.Mean(); err != nil || math.IsNaN(mean) || math.IsInf(mean, 0) || math.Abs(mean-6.4) > 1e-14 {
		t.Fatalf("tail mean %g %v", mean, err)
	}
	if err := stats.Reset(); err != nil {
		t.Fatal(err)
	}
	if count, err := stats.Samples(); err != nil || count != 0 {
		t.Fatalf("reset count %d %v", count, err)
	}
	if weight, err := stats.WeightSum(); err != nil || weight != 0 {
		t.Fatalf("reset weight %g %v", weight, err)
	}
	if table, err := stats.Table(); err != nil || len(table) != 0 {
		t.Fatalf("reset table %v %v", table, err)
	}
	if _, err := stats.Mean(); err == nil {
		t.Fatal("reset mean accepted")
	}
}

func TestConvergenceStatisticsInvalidAtomicity(t *testing.T) {
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
	if err := stats.Add(2, 1); err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct{ values, weights []float64 }{
		{[]float64{4, math.NaN()}, nil}, {[]float64{4, math.Inf(1)}, nil},
		{[]float64{4, 8}, []float64{1, -1}}, {[]float64{4, 8}, []float64{1, math.NaN()}},
		{[]float64{4, 8}, []float64{1, math.Inf(1)}}, {[]float64{4, 8}, []float64{}},
		{make([]float64, 100001), nil}, {[]float64{4, 8}, []float64{math.MaxFloat64, math.MaxFloat64}},
	} {
		if err := stats.AddBatch(test.values, test.weights); err == nil {
			t.Fatal("invalid batch accepted")
		}
		if count, err := stats.Samples(); err != nil || count != 1 {
			t.Fatalf("partial count %d %v", count, err)
		}
		if mean, err := stats.Mean(); err != nil || mean != 2 {
			t.Fatalf("partial mean %g %v", mean, err)
		}
		if table, err := stats.Table(); err != nil || !reflect.DeepEqual(table, []ConvergencePoint{{1, 2}}) {
			t.Fatalf("partial table %v %v", table, err)
		}
	}
	if err := stats.AddBatch(nil, nil); err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct{ values, weights []float64 }{
		{[]float64{1}, []float64{0}}, {[]float64{1}, []float64{}},
		{[]float64{math.NaN()}, nil}, {[]float64{1}, []float64{-1}},
		{make([]float64, 100001), nil},
	} {
		if _, err := StatisticsConvergence(test.values, test.weights); err == nil {
			t.Fatal("invalid batch evaluator accepted")
		}
	}
}

func TestConvergenceStatisticsLifecycleAndIsolation(t *testing.T) {
	first, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer first.Close()
	second, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer second.Close()
	stats, err := first.NewConvergenceStatistics()
	if err != nil {
		t.Fatal(err)
	}
	foreign := &ConvergenceStatistics{object{second, stats.id}}
	if err := foreign.Add(1, 1); err == nil {
		t.Fatal("foreign handle accepted")
	}
	wrong, err := first.NewGeneralStatistics()
	if err != nil {
		t.Fatal(err)
	}
	defer wrong.Close()
	invalid := &ConvergenceStatistics{wrong.object}
	if _, err := invalid.Table(); err == nil {
		t.Fatal("wrong-type handle accepted")
	}
	if err := stats.Close(); err != nil {
		t.Fatal(err)
	}
	if err := stats.Add(1, 1); err == nil {
		t.Fatal("released handle accepted")
	}
	if err := first.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := stats.Table(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
	if _, err := first.NewConvergenceStatistics(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed constructor: %v", err)
	}
}

func TestConvergenceStatisticsConcurrentSnapshots(t *testing.T) {
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
	var group sync.WaitGroup
	for range 8 {
		group.Add(1)
		go func() {
			defer group.Done()
			for range 16 {
				if err := stats.Add(2, 1); err != nil {
					t.Error(err)
					return
				}
				table, err := stats.Table()
				if err != nil {
					t.Error(err)
					return
				}
				for _, point := range table {
					if point.Mean != 2 {
						t.Errorf("inconsistent snapshot %v", table)
						return
					}
				}
			}
		}()
	}
	group.Wait()
	if count, err := stats.Samples(); err != nil || count != 128 {
		t.Fatalf("count %d %v", count, err)
	}
}
