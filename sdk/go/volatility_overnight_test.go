package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"testing"
)

func ohlcOvernightSeries(v OHLCOvernightEstimates) []ChartSeries {
	return []ChartSeries{v.GarmanKlassSigma1, v.GarmanKlassSigma3, v.GarmanKlassSigma6}
}

func TestOHLCOvernightVolatility(t *testing.T) {
	open := []float64{100, 110, 111}
	high := []float64{105, 120, 119}
	low := []float64{95, 105, 102}
	close := []float64{100, 115, 108}
	yearFraction := 1.0 / 252.0
	yearFractions := []float64{1.0 / 365.0, yearFraction, yearFraction}
	overnightFraction := 0.25
	inputs := [][]float64{open, high, low, close, yearFractions}
	copies := make([][]float64, len(inputs))
	for i, input := range inputs {
		copies[i] = append([]float64(nil), input...)
	}

	indexed, err := OHLCOvernightVolatility(open, high, low, close, yearFractions, overnightFraction)
	if err != nil {
		t.Fatal(err)
	}
	constant, err := OHLCOvernightVolatilityConstantFraction(open, high, low, close, yearFraction, overnightFraction)
	if err != nil {
		t.Fatal(err)
	}
	for i, input := range inputs {
		if !reflect.DeepEqual(input, copies[i]) {
			t.Fatalf("input %d changed: %v", i, input)
		}
	}
	want := []float64{2.2159224836472786, 1.8303355611439194, 1.6795672145926133}
	indexedSeries := ohlcOvernightSeries(indexed)
	constantSeries := ohlcOvernightSeries(constant)
	for i, series := range indexedSeries {
		if series.FirstValid != 1 || len(series.Values) != len(open) || series.Values[0] != 0 {
			t.Fatalf("indexed series %d has wrong alignment: %+v", i, series)
		}
		if math.Abs(series.Values[1]-want[i]) > 1e-12 {
			t.Fatalf("indexed series %d: got %.16g, want %.16g", i, series.Values[1], want[i])
		}
		other := constantSeries[i]
		if other.FirstValid != 1 || len(other.Values) != len(open) || other.Values[0] != 0 {
			t.Fatalf("constant series %d has wrong alignment: %+v", i, other)
		}
		for j := range series.Values {
			if math.Abs(series.Values[j]-other.Values[j]) > 1e-12 {
				t.Fatalf("series %d at bar %d: indexed=%g constant=%g", i, j, series.Values[j], other.Values[j])
			}
		}
	}
	encoded, err := json.Marshal(indexed)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(encoded, &fields); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"garman_klass_sigma1", "garman_klass_sigma3", "garman_klass_sigma6"} {
		if len(fields[name]) == 0 {
			t.Fatalf("missing JSON field %s: %s", name, encoded)
		}
	}
}

func TestOHLCOvernightVolatilityEmptyAndShort(t *testing.T) {
	for _, test := range []struct {
		name string
		call func() (OHLCOvernightEstimates, error)
		n    int
	}{
		{"indexed empty", func() (OHLCOvernightEstimates, error) {
			return OHLCOvernightVolatility(nil, nil, nil, nil, nil, 0.25)
		}, 0},
		{"scalar empty", func() (OHLCOvernightEstimates, error) {
			return OHLCOvernightVolatilityConstantFraction(nil, nil, nil, nil, 1.0/252.0, 0.25)
		}, 0},
		{"indexed one", func() (OHLCOvernightEstimates, error) {
			return OHLCOvernightVolatility([]float64{100}, []float64{105}, []float64{95}, []float64{100}, []float64{math.NaN()}, 0.25)
		}, 1},
		{"scalar one", func() (OHLCOvernightEstimates, error) {
			return OHLCOvernightVolatilityConstantFraction([]float64{100}, []float64{105}, []float64{95}, []float64{100}, 1.0/252.0, 0.25)
		}, 1},
	} {
		t.Run(test.name, func(t *testing.T) {
			result, err := test.call()
			if err != nil {
				t.Fatal(err)
			}
			for i, series := range ohlcOvernightSeries(result) {
				if series.FirstValid != test.n || len(series.Values) != test.n {
					t.Fatalf("series %d: %+v", i, series)
				}
				if test.n == 1 && series.Values[0] != 0 {
					t.Fatalf("series %d has nonzero warmup: %+v", i, series)
				}
			}
		})
	}
}

func TestOHLCOvernightVolatilityRejectsInvalidInputs(t *testing.T) {
	valid := []float64{100, 110}
	high := []float64{105, 120}
	low := []float64{95, 105}
	close := []float64{100, 115}
	fractions := []float64{math.NaN(), 1.0 / 252.0}
	for _, inputs := range []struct {
		open, high, low, close []float64
	}{
		{nil, high, low, close},
		{valid, nil, low, close},
		{valid, high, nil, close},
		{valid, high, low, nil},
	} {
		if _, err := OHLCOvernightVolatilityConstantFraction(inputs.open, inputs.high, inputs.low, inputs.close, 1.0/252.0, 0.25); err == nil {
			t.Fatal("accepted mismatched OHLC lengths")
		}
		if _, err := OHLCOvernightVolatility(inputs.open, inputs.high, inputs.low, inputs.close, fractions, 0.25); err == nil {
			t.Fatal("accepted mismatched indexed OHLC lengths")
		}
	}
	if _, err := OHLCOvernightVolatility(valid, high, low, close, nil, 0.25); err == nil {
		t.Fatal("accepted mismatched year fraction length")
	}
	for _, bar := range [][4]float64{
		{0, 120, 105, 115}, {-110, 120, 105, 115},
		{110, math.NaN(), 105, 115}, {110, 120, math.Inf(-1), 115},
		{110, 109, 105, 115}, {110, 120, 116, 115},
		{110, 120, 105, 121}, {104, 120, 105, 115},
	} {
		o, h, l, c := []float64{100, bar[0]}, []float64{105, bar[1]}, []float64{95, bar[2]}, []float64{100, bar[3]}
		if _, err := OHLCOvernightVolatilityConstantFraction(o, h, l, c, 1.0/252.0, 0.25); err == nil {
			t.Fatalf("accepted invalid OHLC bar: %v", bar)
		}
		if _, err := OHLCOvernightVolatility(o, h, l, c, fractions, 0.25); err == nil {
			t.Fatalf("accepted invalid indexed OHLC bar: %v", bar)
		}
	}
	for _, y := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		if _, err := OHLCOvernightVolatility(valid, high, low, close, []float64{math.NaN(), y}, 0.25); err == nil {
			t.Fatalf("accepted invalid indexed year fraction: %g", y)
		}
		if _, err := OHLCOvernightVolatilityConstantFraction(valid, high, low, close, y, 0.25); err == nil {
			t.Fatalf("accepted invalid scalar year fraction: %g", y)
		}
		if _, err := OHLCOvernightVolatilityConstantFraction(nil, nil, nil, nil, y, 0.25); err == nil {
			t.Fatalf("accepted invalid scalar year fraction for empty bars: %g", y)
		}
	}
	for _, f := range []float64{-0.1, 0, 1, math.NaN(), math.Inf(1)} {
		if _, err := OHLCOvernightVolatility(valid, high, low, close, fractions, f); err == nil {
			t.Fatalf("accepted invalid indexed overnight fraction: %g", f)
		}
		if _, err := OHLCOvernightVolatilityConstantFraction(valid, high, low, close, 1.0/252.0, f); err == nil {
			t.Fatalf("accepted invalid scalar overnight fraction: %g", f)
		}
	}
	tooLong := make([]float64, DefaultMaxOutputValues/3+1)
	if _, err := OHLCOvernightVolatilityConstantFraction(tooLong, tooLong, tooLong, tooLong, 1.0/252.0, 0.25); err == nil {
		t.Fatal("accepted output above three-series limit")
	}
}
