package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"strings"
	"testing"
)

func TestChartKeltnerIndependentFixtureAndOwnership(t *testing.T) {
	high := []float64{12, 16, 11, 15, 14, 14}
	low := []float64{10, 14, 9, 13, 12, 14}
	close := []float64{11, 15, 10, 14, 13, 14}
	result, err := ChartKeltnerChannels(high, low, close, 3, 2, 1.5)
	if err != nil {
		t.Fatal(err)
	}
	for channel, values := range []struct {
		got  ChartSeries
		want []float64
	}{
		{result.Center, []float64{0, 0, 12, 13, 13, 13.5}},
		{result.Upper, []float64{0, 0, 153.0 / 8, 325.0 / 16, 581.0 / 32, 1077.0 / 64}},
		{result.Lower, []float64{0, 0, 39.0 / 8, 91.0 / 16, 251.0 / 32, 651.0 / 64}},
	} {
		if values.got.FirstValid != 2 || len(values.got.Values) != 6 || values.got.NullableValues()[1] != nil {
			t.Fatalf("bad channel %d alignment", channel)
		}
		for i, want := range values.want {
			if math.Abs(values.got.Values[i]-want) > 1e-12 {
				t.Fatalf("channel %d bar %d=%g want %g", channel, i, values.got.Values[i], want)
			}
		}
	}
	encoded, err := json.Marshal(result)
	if err != nil || strings.Count(string(encoded), `null,null`) != 3 {
		t.Fatalf("combined warmup JSON lost: %s %v", encoded, err)
	}
	upper := append([]float64(nil), result.Upper.Values...)
	result.Center.Values = append(result.Center.Values, 99)
	result.Center.Values[2] = 99
	if !reflect.DeepEqual(result.Upper.Values, upper) {
		t.Fatal("channel slice capacity leaked into another channel")
	}
	high[0], low[0], close[0] = -99, -99, -99
	if !reflect.DeepEqual(result.Upper.Values, upper) {
		t.Fatal("result retained caller arrays")
	}
	fresh, err := ChartKeltnerChannels([]float64{12, 16, 11}, []float64{10, 14, 9}, []float64{11, 15, 10}, 3, 2, 1.5)
	if err != nil || fresh.Center.Values[2] != 12 {
		t.Fatal("result ownership leaked across calls")
	}
}

func TestChartKeltnerDefaultsEmptyShortFlatAndZero(t *testing.T) {
	empty, err := DefaultKeltnerChannels(nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, series := range []ChartSeries{empty.Center, empty.Upper, empty.Lower} {
		if len(series.Values) != 0 || series.FirstValid != 0 {
			t.Fatal("bad empty alignment")
		}
	}
	short, err := ChartKeltnerChannels([]float64{2}, []float64{0}, []float64{1}, 1, 3, 2)
	if err != nil {
		t.Fatal(err)
	}
	for _, series := range []ChartSeries{short.Center, short.Upper, short.Lower} {
		if series.FirstValid != 1 || !reflect.DeepEqual(series.Values, []float64{0}) {
			t.Fatal("bad short alignment")
		}
	}
	close := []float64{1, 2, 3, 4, 5}
	for _, periods := range [][2]int{{2, 4}, {4, 2}} {
		result, err := ChartKeltnerChannels(close, close, close, periods[0], periods[1], 0)
		if err != nil || result.Center.FirstValid != 3 || !reflect.DeepEqual(result.Center, result.Upper) || !reflect.DeepEqual(result.Center, result.Lower) {
			t.Fatalf("bad zero-width warmup %+v %v", result, err)
		}
	}
	flat, err := ChartKeltnerChannels([]float64{-3, -3, -3}, []float64{-3, -3, -3}, []float64{-3, -3, -3}, 2, 1, 2)
	if err != nil || !reflect.DeepEqual(flat.Center.Values, []float64{0, -3, -3}) || !reflect.DeepEqual(flat.Center, flat.Upper) || !reflect.DeepEqual(flat.Center, flat.Lower) {
		t.Fatalf("bad flat negative prices: %+v %v", flat, err)
	}
	close = make([]float64, 24)
	for i := range close {
		close[i] = float64(i + 1)
	}
	defaults, err := DefaultKeltnerChannels(close, close, close)
	if err != nil {
		t.Fatal(err)
	}
	explicit, err := ChartKeltnerChannels(close, close, close, 20, 10, 2)
	if err != nil || !reflect.DeepEqual(defaults, explicit) || defaults.Center.FirstValid != 19 {
		t.Fatalf("bad defaults %+v %v", defaults, err)
	}
}

func TestChartKeltnerRejectParametersInputsAndOverflow(t *testing.T) {
	for _, params := range []struct {
		center, atr int
		mult        float64
	}{{0, 10, 2}, {20, 0, 2}, {-1, 10, 2}, {20, -1, 2}, {20, 10, -1}, {20, 10, math.NaN()}, {20, 10, math.Inf(1)}, {20, 10, math.Inf(-1)}} {
		if got, err := ChartKeltnerChannels(nil, nil, nil, params.center, params.atr, params.mult); err == nil || got.Center.Values != nil {
			t.Fatal("accepted invalid parameter")
		}
	}
	for _, values := range []struct{ high, low, close []float64 }{
		{nil, []float64{0}, []float64{1}}, {[]float64{2}, nil, []float64{1}}, {[]float64{2}, []float64{0}, nil},
		{[]float64{math.NaN()}, []float64{0}, []float64{1}}, {[]float64{2}, []float64{math.Inf(-1)}, []float64{1}}, {[]float64{2}, []float64{0}, []float64{math.Inf(1)}},
		{[]float64{2}, []float64{3}, []float64{1}}, {[]float64{math.MaxFloat64}, []float64{-math.MaxFloat64}, []float64{0}},
		{[]float64{-math.MaxFloat64, math.MaxFloat64}, []float64{-math.MaxFloat64, 0}, []float64{-math.MaxFloat64, 0}},
	} {
		if _, err := ChartKeltnerChannels(values.high, values.low, values.close, 20, 10, 0); err == nil {
			t.Fatal("accepted invalid HLC during warmup")
		}
	}
	for _, values := range []struct{ high, low, close, mult float64 }{{math.MaxFloat64, 0, 0, 2}, {math.MaxFloat64, 0, math.MaxFloat64, 1}, {0, -math.MaxFloat64, -math.MaxFloat64, 1}} {
		got, err := ChartKeltnerChannels([]float64{values.high}, []float64{values.low}, []float64{values.close}, 1, 1, values.mult)
		if err == nil || got.Center.Values != nil || got.Upper.Values != nil || got.Lower.Values != nil {
			t.Fatal("accepted band/offset overflow or leaked partial output")
		}
	}
	large := make([]float64, DefaultMaxOutputValues/3+1)
	if _, err := DefaultKeltnerChannels(large, large, large); err == nil {
		t.Fatal("accepted excessive combined output")
	}
	zero, err := ChartKeltnerChannels([]float64{math.MaxFloat64}, []float64{0}, []float64{math.MaxFloat64}, 1, 1, 0)
	if err != nil || zero.Center.Values[0] != math.MaxFloat64 || !reflect.DeepEqual(zero.Center, zero.Upper) || !reflect.DeepEqual(zero.Center, zero.Lower) {
		t.Fatal("zero multiplier lost finite extreme center")
	}
}

func TestChartKeltnerCenterUsesCloseNotTypicalPrice(t *testing.T) {
	result, err := ChartKeltnerChannels([]float64{9, 12, 15}, []float64{0, 3, 6}, []float64{0, 3, 6}, 3, 2, 0)
	if err != nil || result.Center.Values[2] != 3 {
		t.Fatalf("expected close seed3, not typical-price6: %+v %v", result, err)
	}
}
