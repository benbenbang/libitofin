package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"testing"
)

func ohlcPointSeries(v OHLCPointEstimates) []ChartSeries {
	return []ChartSeries{v.SimpleSigma, v.ParkinsonSigma, v.GarmanKlassSigma4, v.GarmanKlassSigma5}
}

func TestOHLCPointVolatility(t *testing.T) {
	open := []float64{100, 101}
	high := []float64{110, 116}
	low := []float64{90, 98}
	close := []float64{105, 109}
	yearFraction := 1.0 / 252.0
	yearFractions := []float64{yearFraction, yearFraction}
	inputs := [][]float64{open, high, low, close, yearFractions}
	copies := make([][]float64, len(inputs))
	for i, input := range inputs {
		copies[i] = append([]float64(nil), input...)
	}

	indexed, err := OHLCPointVolatility(open, high, low, close, yearFractions)
	if err != nil {
		t.Fatal(err)
	}
	constant, err := OHLCPointVolatilityConstantFraction(open, high, low, close, yearFraction)
	if err != nil {
		t.Fatal(err)
	}
	for i, input := range inputs {
		if !reflect.DeepEqual(input, copies[i]) {
			t.Fatalf("input %d changed: %v", i, input)
		}
	}
	want := []float64{0.7745198449099887, 1.9131168640323526, 2.204975405342331, 2.2004838300550182}
	for i, series := range ohlcPointSeries(indexed) {
		if series.FirstValid != 0 || len(series.Values) != len(open) {
			t.Fatalf("indexed series %d has wrong alignment: %+v", i, series)
		}
		if math.Abs(series.Values[0]-want[i]) > 1e-12 {
			t.Fatalf("indexed series %d: got %.16g, want %.16g", i, series.Values[0], want[i])
		}
		other := ohlcPointSeries(constant)[i]
		if other.FirstValid != 0 || len(other.Values) != len(open) {
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
	for _, name := range []string{"simple_sigma", "parkinson_sigma", "garman_klass_sigma4", "garman_klass_sigma5"} {
		if len(fields[name]) == 0 {
			t.Fatalf("missing JSON field %s: %s", name, encoded)
		}
	}
}

func TestOHLCPointVolatilityEmptyAndShort(t *testing.T) {
	for _, estimates := range []struct {
		name string
		call func() (OHLCPointEstimates, error)
		n    int
	}{
		{"indexed empty", func() (OHLCPointEstimates, error) { return OHLCPointVolatility(nil, nil, nil, nil, nil) }, 0},
		{"scalar empty", func() (OHLCPointEstimates, error) {
			return OHLCPointVolatilityConstantFraction(nil, nil, nil, nil, 1.0/252.0)
		}, 0},
		{"indexed one", func() (OHLCPointEstimates, error) {
			return OHLCPointVolatility([]float64{100}, []float64{110}, []float64{90}, []float64{105}, []float64{1.0 / 252.0})
		}, 1},
		{"scalar one", func() (OHLCPointEstimates, error) {
			return OHLCPointVolatilityConstantFraction([]float64{100}, []float64{110}, []float64{90}, []float64{105}, 1.0/252.0)
		}, 1},
	} {
		t.Run(estimates.name, func(t *testing.T) {
			result, err := estimates.call()
			if err != nil {
				t.Fatal(err)
			}
			for i, series := range ohlcPointSeries(result) {
				if series.FirstValid != 0 || len(series.Values) != estimates.n {
					t.Fatalf("series %d: %+v", i, series)
				}
			}
		})
	}
}

func TestOHLCPointVolatilityRejectsInvalidInputs(t *testing.T) {
	valid := []float64{100}
	high := []float64{110}
	low := []float64{90}
	close := []float64{105}
	fraction := 1.0 / 252.0
	for _, inputs := range []struct {
		open, high, low, close []float64
	}{
		{nil, high, low, close},
		{valid, nil, low, close},
		{valid, high, nil, close},
		{valid, high, low, nil},
	} {
		if _, err := OHLCPointVolatilityConstantFraction(inputs.open, inputs.high, inputs.low, inputs.close, fraction); err == nil {
			t.Fatal("accepted mismatched OHLC lengths")
		}
		if _, err := OHLCPointVolatility(inputs.open, inputs.high, inputs.low, inputs.close, []float64{fraction}); err == nil {
			t.Fatal("accepted mismatched indexed OHLC lengths")
		}
	}
	if _, err := OHLCPointVolatility(valid, high, low, close, nil); err == nil {
		t.Fatal("accepted mismatched year fraction length")
	}
	for _, bar := range [][4]float64{
		{0, 110, 90, 105}, {-100, 110, 90, 105},
		{100, math.NaN(), 90, 105}, {100, 110, math.Inf(-1), 105},
		{100, 99, 90, 105}, {100, 110, 106, 105},
		{100, 110, 90, 111}, {89, 110, 90, 105},
	} {
		o, h, l, c := []float64{bar[0]}, []float64{bar[1]}, []float64{bar[2]}, []float64{bar[3]}
		if _, err := OHLCPointVolatilityConstantFraction(o, h, l, c, fraction); err == nil {
			t.Fatalf("accepted invalid OHLC bar: %v", bar)
		}
		if _, err := OHLCPointVolatility(o, h, l, c, []float64{fraction}); err == nil {
			t.Fatalf("accepted invalid indexed OHLC bar: %v", bar)
		}
	}
	for _, y := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		if _, err := OHLCPointVolatility(valid, high, low, close, []float64{y}); err == nil {
			t.Fatalf("accepted invalid indexed year fraction: %g", y)
		}
		if _, err := OHLCPointVolatilityConstantFraction(valid, high, low, close, y); err == nil {
			t.Fatalf("accepted invalid scalar year fraction: %g", y)
		}
		if _, err := OHLCPointVolatilityConstantFraction(nil, nil, nil, nil, y); err == nil {
			t.Fatalf("accepted invalid scalar year fraction for empty bars: %g", y)
		}
	}
	tooLong := make([]float64, DefaultMaxOutputValues/4+1)
	if _, err := OHLCPointVolatilityConstantFraction(tooLong, tooLong, tooLong, tooLong, fraction); err == nil {
		t.Fatal("accepted output above four-series limit")
	}
}
