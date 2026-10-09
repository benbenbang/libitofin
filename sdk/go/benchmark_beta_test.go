package itofin

import (
	"math"
	"os"
	"reflect"
	"strconv"
	"strings"
	"testing"
)

func TestBenchmarkBetaIndependentFixtures(t *testing.T) {
	data, err := os.ReadFile("testdata/benchmark-beta.csv")
	if err != nil {
		t.Fatal(err)
	}
	parse := func(s string) []float64 {
		if s == "" {
			return nil
		}
		var values []float64
		for _, cell := range strings.Split(s, ",") {
			v, e := strconv.ParseFloat(cell, 64)
			if e != nil {
				t.Fatal(e)
			}
			values = append(values, v)
		}
		return values
	}
	for _, line := range strings.Split(strings.TrimSpace(string(data)), "\n")[1:] {
		cells := strings.Split(line, ";")
		a, b, w := parse(cells[1]), parse(cells[2]), parse(cells[3])
		originalA, originalB := append([]float64(nil), a...), append([]float64(nil), b...)
		expected, err := strconv.ParseFloat(cells[4], 64)
		if err != nil {
			t.Fatal(err)
		}
		for i := 0; i < 10; i++ {
			actual, err := BenchmarkBeta(a, b, w)
			if err != nil || math.Abs(actual-expected) > 2e-12 {
				t.Fatalf("%s: %g != %g, %v", cells[0], actual, expected, err)
			}
		}
		if !reflect.DeepEqual(a, originalA) || !reflect.DeepEqual(b, originalB) {
			t.Fatal("inputs changed")
		}
	}
}

func TestBenchmarkBetaErrors(t *testing.T) {
	for _, tc := range []struct{ a, b, w []float64 }{
		{nil, nil, nil}, {[]float64{1}, []float64{2}, nil},
		{[]float64{1, 2}, []float64{1}, nil},
		{[]float64{1, 2}, []float64{3, 3}, nil},
		{[]float64{1, 2}, []float64{1, 2}, []float64{}},
		{[]float64{1, 2}, []float64{1, 2}, []float64{1}},
		{[]float64{1, 2}, []float64{1, 2}, []float64{0, 0}},
		{[]float64{1, 2}, []float64{1, 2}, []float64{-1, 2}},
		{[]float64{math.NaN(), 2}, []float64{1, 2}, []float64{0, 1}},
		{[]float64{1, 2}, []float64{math.Inf(1), 2}, nil},
		{[]float64{1, 2}, []float64{1, 2}, []float64{math.NaN(), 1}},
		{[]float64{1, 2}, []float64{1, 2}, []float64{math.Inf(1), 1}},
		{[]float64{1, 2}, []float64{1, 2}, []float64{math.MaxFloat64, math.MaxFloat64}},
		{[]float64{math.MaxFloat64, -math.MaxFloat64}, []float64{1, 2}, nil},
		{[]float64{1, 2}, []float64{1e-200, 2e-200}, nil},
		{make([]float64, 100001), make([]float64, 100001), nil},
	} {
		value, err := BenchmarkBeta(tc.a, tc.b, tc.w)
		if err == nil || value != 0 {
			t.Fatalf("expected zero and error, got %g %v", value, err)
		}
	}
}
