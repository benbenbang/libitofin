package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"testing"
)

func TestChartATRTrueRangeHandFixture(t *testing.T) {
	high := []float64{12, 16, 11, 15, 14, 14}
	low := []float64{10, 14, 9, 13, 12, 14}
	close := []float64{11, 15, 10, 14, 13, 14}
	ranges, err := TrueRange(high, low, close)
	if err != nil || ranges.FirstValid != 0 || !reflect.DeepEqual(ranges.Values, []float64{2, 5, 6, 5, 2, 1}) {
		t.Fatalf("unexpected true range: %+v, %v", ranges, err)
	}
	result, err := ATR(high, low, close, 3)
	if err != nil || result.FirstValid != 2 || len(result.Values) != 6 {
		t.Fatalf("unexpected ATR alignment: %+v, %v", result, err)
	}
	for i, want := range []float64{0, 0, 13.0 / 3, 41.0 / 9, 100.0 / 27, 227.0 / 81} {
		if math.Abs(result.Values[i]-want) > 1e-12 {
			t.Fatalf("ATR[%d]=%g want %g", i, result.Values[i], want)
		}
	}
	if result.NullableValues()[0] != nil || result.NullableValues()[1] != nil || result.NullableValues()[2] == nil {
		t.Fatal("ATR warmup contract lost")
	}
	high[0], low[0], close[0] = -99, -99, -99
	if result.Values[2] == -99 {
		t.Fatal("result retained inputs")
	}
	result.Values[2] = -99
	fresh, err := ATR([]float64{12, 16, 11}, []float64{10, 14, 9}, []float64{11, 15, 10}, 3)
	if err != nil || math.Abs(fresh.Values[2]-13.0/3) > 1e-12 {
		t.Fatal("result ownership leaked across calls")
	}
}

func TestChartATREmptyShortDefaultsAndPeriodOne(t *testing.T) {
	for _, fn := range []func([]float64, []float64, []float64) (ChartSeries, error){TrueRange, DefaultATR} {
		result, err := fn(nil, nil, nil)
		if err != nil || len(result.Values) != 0 || result.FirstValid != 0 {
			t.Fatalf("unexpected empty result: %+v %v", result, err)
		}
	}
	short, err := ATR([]float64{2}, []float64{0}, []float64{1}, 3)
	if err != nil || short.FirstValid != 1 {
		t.Fatalf("unexpected short result: %+v %v", short, err)
	}
	encoded, err := json.Marshal(short)
	if err != nil || string(encoded) != `{"values":[null],"first_valid":1}` {
		t.Fatalf("unexpected warmup JSON: %s %v", encoded, err)
	}
	high, low, close := make([]float64, 15), make([]float64, 15), make([]float64, 15)
	for i := range high {
		high[i] = float64(i + 1)
	}
	result, err := DefaultATR(high, low, close)
	if err != nil || result.FirstValid != 13 || result.Values[13] != 7.5 || math.Abs(result.Values[14]-225.0/28) > 1e-12 {
		t.Fatalf("unexpected default ATR: %+v %v", result, err)
	}
	one, err := ATR([]float64{math.MaxFloat64, math.SmallestNonzeroFloat64}, []float64{0, 0}, []float64{0, 0}, 1)
	if err != nil || one.Values[1] != math.SmallestNonzeroFloat64 {
		t.Fatalf("period-one cancellation: %+v %v", one, err)
	}
	maximum, err := ATR([]float64{math.MaxFloat64, math.MaxFloat64, math.MaxFloat64}, []float64{0, 0, 0}, []float64{0, 0, 0}, 3)
	if err != nil || maximum.Values[2] != math.MaxFloat64 {
		t.Fatalf("representable seed overflow: %+v %v", maximum, err)
	}
}

func TestChartATRTrueRangeRejectInvalidAndOverflow(t *testing.T) {
	for _, values := range []struct{ high, low, close []float64 }{
		{nil, []float64{0}, []float64{1}},
		{[]float64{2}, nil, []float64{1}},
		{[]float64{2}, []float64{0}, nil},
		{[]float64{math.NaN()}, []float64{0}, []float64{1}},
		{[]float64{2}, []float64{math.Inf(-1)}, []float64{1}},
		{[]float64{2}, []float64{0}, []float64{math.Inf(1)}},
		{[]float64{2}, []float64{3}, []float64{1}},
		{[]float64{math.MaxFloat64}, []float64{-math.MaxFloat64}, []float64{0}},
		{[]float64{-math.MaxFloat64, math.MaxFloat64}, []float64{-math.MaxFloat64, 0}, []float64{-math.MaxFloat64, 0}},
		{[]float64{math.MaxFloat64, 0}, []float64{math.MaxFloat64, -math.MaxFloat64}, []float64{math.MaxFloat64, 0}},
	} {
		if got, err := TrueRange(values.high, values.low, values.close); err == nil || got.Values != nil {
			t.Fatal("TrueRange accepted invalid inputs")
		}
		if got, err := ATR(values.high, values.low, values.close, 14); err == nil || got.Values != nil {
			t.Fatal("ATR accepted invalid inputs during warmup")
		}
	}
	for _, period := range []int{0, -1} {
		if _, err := ATR(nil, nil, nil, period); err == nil {
			t.Fatal("ATR accepted invalid period")
		}
	}
	large := make([]float64, DefaultMaxOutputValues+1)
	if _, err := TrueRange(large, large, large); err == nil {
		t.Fatal("TrueRange accepted excessive output")
	}
	if _, err := ATR(large, large, large, 14); err == nil {
		t.Fatal("ATR accepted excessive output")
	}
	flat, err := ATR([]float64{-3, -3, -3}, []float64{-3, -3, -3}, []float64{-3, -3, -3}, 2)
	if err != nil || !reflect.DeepEqual(flat.Values, []float64{0, 0, 0}) {
		t.Fatalf("unexpected flat result: %+v %v", flat, err)
	}
}
