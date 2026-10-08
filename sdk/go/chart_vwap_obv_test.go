package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"testing"
)

func TestChartVWAPOBVHandFixture(t *testing.T) {
	price := []float64{10, 12, 11, 11, 9}
	volume := []float64{0, 2, 1, 0, 3}
	originalPrice := append([]float64(nil), price...)
	originalVolume := append([]float64(nil), volume...)
	vwap, err := VWAP(price, volume)
	if err != nil {
		t.Fatal(err)
	}
	if vwap.FirstValid != 1 || len(vwap.Values) != len(price) {
		t.Fatalf("unexpected VWAP alignment: %+v", vwap)
	}
	for i, want := range []float64{0, 12, 35.0 / 3, 35.0 / 3, 31.0 / 3} {
		if math.Abs(vwap.Values[i]-want) > 1e-12 {
			t.Fatalf("VWAP[%d]=%g want %g", i, vwap.Values[i], want)
		}
	}
	if vwap.NullableValues()[0] != nil || *vwap.NullableValues()[1] != 12 {
		t.Fatal("VWAP missing prefix was lost")
	}
	obv, err := OBV(price, volume)
	if err != nil || obv.FirstValid != 0 || !reflect.DeepEqual(obv.Values, []float64{0, 2, 1, 1, -2}) {
		t.Fatalf("unexpected OBV: %+v, %v", obv, err)
	}
	if !reflect.DeepEqual(price, originalPrice) || !reflect.DeepEqual(volume, originalVolume) {
		t.Fatal("chart function mutated inputs")
	}
	vwap.Values[1] = -1
	fresh, err := VWAP(price, volume)
	if err != nil || fresh.Values[1] != 12 {
		t.Fatal("result ownership leaked across calls")
	}
}

func TestChartVWAPOBVEmptyZeroAndSessionReset(t *testing.T) {
	for _, fn := range []func([]float64, []float64) (ChartSeries, error){VWAP, OBV} {
		empty, err := fn(nil, nil)
		if err != nil || len(empty.Values) != 0 || empty.FirstValid != 0 {
			t.Fatalf("unexpected empty result: %+v, %v", empty, err)
		}
	}
	missing, err := VWAP([]float64{1, 2}, []float64{0, 0})
	if err != nil || missing.FirstValid != 2 {
		t.Fatalf("unexpected missing result: %+v, %v", missing, err)
	}
	encoded, err := json.Marshal(missing)
	if err != nil || string(encoded) != `{"values":[null,null],"first_valid":2}` {
		t.Fatalf("unexpected missing JSON: %s, %v", encoded, err)
	}
	carry, err := VWAP([]float64{100, 3, 999}, []float64{0, 2, 0})
	if err != nil || carry.FirstValid != 1 || !reflect.DeepEqual(carry.Values, []float64{0, 3, 3}) {
		t.Fatalf("unexpected zero-volume carry: %+v, %v", carry, err)
	}
	reset, err := VWAP([]float64{999}, []float64{2})
	if err != nil || reset.FirstValid != 0 || reset.Values[0] != 999 {
		t.Fatalf("session did not reset: %+v, %v", reset, err)
	}
	seed, err := OBV([]float64{1, 1, 2, 0}, []float64{999, 99, 0, 3})
	if err != nil || !reflect.DeepEqual(seed.Values, []float64{0, 0, 0, -3}) {
		t.Fatalf("unexpected OBV seed/equal-close/zero-volume values: %+v, %v", seed, err)
	}
}

func TestChartVWAPOBVRejectInvalidAndOverflow(t *testing.T) {
	for _, fn := range []func([]float64, []float64) (ChartSeries, error){VWAP, OBV} {
		for _, values := range []struct{ price, volume []float64 }{
			{nil, []float64{1}},
			{[]float64{1}, nil},
			{[]float64{math.NaN()}, []float64{0}},
			{[]float64{math.Inf(1)}, []float64{0}},
			{[]float64{1}, []float64{math.NaN()}},
			{[]float64{1}, []float64{math.Inf(-1)}},
			{[]float64{1}, []float64{-1}},
		} {
			if got, err := fn(values.price, values.volume); err == nil || got.Values != nil {
				t.Fatalf("invalid input did not return empty result and error: %+v, %v", got, err)
			}
		}
	}
	if _, err := VWAP([]float64{0, 0}, []float64{math.MaxFloat64, math.MaxFloat64}); err == nil {
		t.Fatal("VWAP accepted cumulative-volume overflow")
	}
	for _, price := range [][]float64{{1, 2, 3}, {3, 2, 1}} {
		if _, err := OBV(price, []float64{0, math.MaxFloat64, math.MaxFloat64}); err == nil {
			t.Fatal("OBV accepted signed-volume overflow")
		}
	}
	large := make([]float64, DefaultMaxOutputValues+1)
	for _, fn := range []func([]float64, []float64) (ChartSeries, error){VWAP, OBV} {
		if _, err := fn(large, large); err == nil {
			t.Fatal("chart accepted input exceeding output allocation limit")
		}
	}
}

func TestChartVWAPOBVFiniteExtremes(t *testing.T) {
	for _, values := range []struct {
		price, volume []float64
		want          float64
	}{
		{[]float64{math.MaxFloat64, math.MaxFloat64}, []float64{2, 2}, math.MaxFloat64},
		{[]float64{-math.MaxFloat64, math.MaxFloat64}, []float64{1, 1}, 0},
		{[]float64{3, 5}, []float64{math.SmallestNonzeroFloat64, math.SmallestNonzeroFloat64}, 4},
		{[]float64{math.MaxFloat64, 0}, []float64{1, math.MaxFloat64}, 1},
	} {
		got, err := VWAP(values.price, values.volume)
		if err != nil || math.Abs(got.Values[len(got.Values)-1]-values.want) > 1e-12 {
			t.Fatalf("unexpected finite-extreme VWAP: %+v, %v", got, err)
		}
	}
	got, err := OBV([]float64{-math.MaxFloat64, math.MaxFloat64, -math.MaxFloat64}, []float64{1, 2, 3})
	if err != nil || !reflect.DeepEqual(got.Values, []float64{0, 2, -1}) {
		t.Fatalf("extreme closes must be compared without subtracting: %+v, %v", got, err)
	}
}
