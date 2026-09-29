package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"strings"
	"testing"
)

func TestChartAveragesAndWarmup(t *testing.T) {
	close := []float64{1, 2, 3, 4}
	for _, average := range []struct {
		name string
		fn   func([]float64, int) (ChartSeries, error)
	}{
		{"SMA", SMA},
		{"EMA", EMA},
	} {
		t.Run(average.name, func(t *testing.T) {
			got, err := average.fn(close, 3)
			if err != nil {
				t.Fatal(err)
			}
			if got.FirstValid != 2 || !reflect.DeepEqual(got.Values, []float64{0, 0, 2, 3}) {
				t.Fatalf("unexpected aligned values: %+v", got)
			}
			encoded, err := json.Marshal(got)
			if err != nil {
				t.Fatal(err)
			}
			if string(encoded) != `{"values":[null,null,2,3],"first_valid":2}` {
				t.Fatalf("unexpected JSON: %s", encoded)
			}
			nullable := got.NullableValues()
			if nullable[0] != nil || nullable[1] != nil || nullable[2] == nil || *nullable[2] != 2 {
				t.Fatalf("unexpected nullable values: %+v", nullable)
			}
		})
	}
	short, err := EMA([]float64{1}, 2)
	if err != nil || short.FirstValid != 1 || !reflect.DeepEqual(short.Values, []float64{0}) {
		t.Fatalf("short EMA: %+v, %v", short, err)
	}
	empty, err := SMA(nil, 2)
	if err != nil || empty.FirstValid != 0 || len(empty.Values) != 0 {
		t.Fatalf("empty SMA: %+v, %v", empty, err)
	}
	simple, err := SMA([]float64{1, 2, 3, 10}, 3)
	if err != nil {
		t.Fatal(err)
	}
	exponential, err := EMA([]float64{1, 2, 3, 10}, 3)
	if err != nil {
		t.Fatal(err)
	}
	if simple.Values[3] != 5 || exponential.Values[3] != 6 {
		t.Fatalf("SMA and EMA did not diverge after seeding: %v, %v", simple.Values, exponential.Values)
	}
}

func TestChartRejectsInvalidInputs(t *testing.T) {
	if _, err := SMA([]float64{1}, 0); err == nil {
		t.Fatal("accepted zero period")
	}
	if _, err := EMA([]float64{math.NaN()}, 1); err == nil {
		t.Fatal("accepted nonfinite close")
	}
	if _, err := ChartVolumeBars([]float64{1}, nil, nil, nil, nil); err == nil {
		t.Fatal("accepted mismatched OHLCV lengths")
	}
	if _, err := ChartVolumeBars([]float64{1}, []float64{2}, []float64{0}, []float64{1}, []float64{-1}); err == nil {
		t.Fatal("accepted negative volume")
	}
	if _, err := ChartVolumeBars([]float64{1}, []float64{2}, []float64{0}, []float64{3}, []float64{1}); err == nil {
		t.Fatal("accepted close outside high/low")
	}
}

func TestChartVolumeBars(t *testing.T) {
	got, err := ChartVolumeBars(
		[]float64{1, 2, -2}, []float64{3, 3, 0}, []float64{0, 0, -3},
		[]float64{2, 1, -2}, []float64{10, 11, 0},
	)
	if err != nil {
		t.Fatal(err)
	}
	if got.Volume.FirstValid != 0 || !reflect.DeepEqual(got.Volume.Values, []float64{10, 11, 0}) || !reflect.DeepEqual(got.Direction, []int8{1, -1, 0}) {
		t.Fatalf("unexpected volume bars: %+v", got)
	}
	empty, err := ChartVolumeBars(nil, nil, nil, nil, nil)
	if err != nil || len(empty.Volume.Values) != 0 || len(empty.Direction) != 0 {
		t.Fatalf("empty volume bars: %+v, %v", empty, err)
	}
}

func TestChartBollingerBands(t *testing.T) {
	got, err := ChartBollingerBands([]float64{1, 2, 3, 4}, 3, 2)
	if err != nil {
		t.Fatal(err)
	}
	if got.Middle.FirstValid != 2 || got.Upper.FirstValid != 2 || got.Lower.FirstValid != 2 {
		t.Fatalf("unexpected band warmup: %+v", got)
	}
	if !reflect.DeepEqual(got.Middle.Values, []float64{0, 0, 2, 3}) {
		t.Fatalf("unexpected band midpoint: %v", got.Middle.Values)
	}
	offset := 2 * math.Sqrt(2.0/3.0)
	for i := 2; i < 4; i++ {
		if math.Abs(got.Upper.Values[i]-(got.Middle.Values[i]+offset)) > 1e-12 ||
			math.Abs(got.Lower.Values[i]-(got.Middle.Values[i]-offset)) > 1e-12 {
			t.Fatalf("unexpected band at %d: %+v", i, got)
		}
	}
	encoded, err := json.Marshal(got.Upper)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(string(encoded), `{"values":[null,null,`) {
		t.Fatalf("band warmup did not serialize as null: %s", encoded)
	}
	short, err := ChartBollingerBands([]float64{1}, 3, 2)
	if err != nil || short.Middle.FirstValid != 1 || short.Upper.NullableValues()[0] != nil {
		t.Fatalf("short bands: %+v, %v", short, err)
	}
	defaults, err := DefaultBollingerBands(make([]float64, 20))
	if err != nil || defaults.Middle.FirstValid != 19 {
		t.Fatalf("default bands: %+v, %v", defaults, err)
	}
}

func TestChartRSI(t *testing.T) {
	got, err := RSI([]float64{1, 2, 3, 2, 2}, 2)
	if err != nil {
		t.Fatal(err)
	}
	if got.FirstValid != 2 || !reflect.DeepEqual(got.Values, []float64{0, 0, 100, 50, 50}) {
		t.Fatalf("unexpected Wilder RSI: %+v", got)
	}
	encoded, err := json.Marshal(got)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"values":[null,null,100,50,50],"first_valid":2}` {
		t.Fatalf("unexpected RSI JSON: %s", encoded)
	}
	flat, err := RSI([]float64{4, 4, 4}, 2)
	if err != nil || flat.Values[2] != 50 {
		t.Fatalf("flat RSI: %+v, %v", flat, err)
	}
	short, err := RSI([]float64{1}, 2)
	if err != nil || short.FirstValid != 1 || short.NullableValues()[0] != nil {
		t.Fatalf("short RSI: %+v, %v", short, err)
	}
	defaults, err := DefaultRSI(make([]float64, 15))
	if err != nil || defaults.FirstValid != 14 || defaults.Values[14] != 50 {
		t.Fatalf("default RSI: %+v, %v", defaults, err)
	}
}

func TestChartBandsAndRSIRejectInvalidInputs(t *testing.T) {
	for _, input := range []struct {
		period     int
		multiplier float64
	}{
		{0, 2}, {2, -1}, {2, math.NaN()}, {2, math.Inf(1)},
	} {
		if _, err := ChartBollingerBands([]float64{1, 2}, input.period, input.multiplier); err == nil {
			t.Fatalf("accepted invalid bands parameter: %+v", input)
		}
	}
	if _, err := ChartBollingerBands([]float64{math.NaN()}, 2, 2); err == nil {
		t.Fatal("accepted nonfinite band close")
	}
	if _, err := RSI(nil, 0); err == nil {
		t.Fatal("accepted zero RSI period")
	}
	if _, err := RSI([]float64{math.Inf(1)}, 2); err == nil {
		t.Fatal("accepted nonfinite RSI close")
	}
}

func TestChartKD(t *testing.T) {
	got, err := ChartKD(
		[]float64{12, 14, 16, 18}, []float64{8, 10, 12, 14},
		[]float64{10, 12, 14, 16}, 3, 3, 3,
	)
	if err != nil {
		t.Fatal(err)
	}
	if got.RSV.FirstValid != 2 || got.K.FirstValid != 2 || got.D.FirstValid != 2 ||
		!reflect.DeepEqual(got.RSV.Values, []float64{0, 0, 75, 75}) {
		t.Fatalf("unexpected KD alignment: %+v", got)
	}
	if math.Abs(got.K.Values[2]-58.333333333333336) > 1e-12 ||
		math.Abs(got.D.Values[2]-52.77777777777778) > 1e-12 ||
		math.Abs(got.K.Values[3]-63.88888888888889) > 1e-12 ||
		math.Abs(got.D.Values[3]-56.48148148148148) > 1e-12 {
		t.Fatalf("unexpected Taiwan KD smoothing: %+v", got)
	}
	encoded, err := json.Marshal(got.K)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(string(encoded), `{"values":[null,null,`) {
		t.Fatalf("KD warmup did not serialize as null: %s", encoded)
	}
	flat, err := ChartKD([]float64{2, 2, 2}, []float64{2, 2, 2}, []float64{2, 2, 2}, 2, 3, 3)
	if err != nil || flat.RSV.Values[1] != 50 || flat.K.Values[1] != 50 || flat.D.Values[2] != 50 {
		t.Fatalf("flat KD: %+v, %v", flat, err)
	}
	defaults, err := DefaultKD(make([]float64, 9), make([]float64, 9), make([]float64, 9))
	if err != nil || defaults.RSV.FirstValid != 8 || defaults.K.Values[8] != 50 {
		t.Fatalf("default KD: %+v, %v", defaults, err)
	}
	empty, err := DefaultKD(nil, nil, nil)
	if err != nil || empty.RSV.FirstValid != 0 || len(empty.K.Values) != 0 {
		t.Fatalf("empty KD: %+v, %v", empty, err)
	}
}

func TestChartMACD(t *testing.T) {
	got, err := ChartMACD([]float64{1, 2, 3, 4}, 2, 3, 2)
	if err != nil {
		t.Fatal(err)
	}
	if got.Line.FirstValid != 2 || got.Signal.FirstValid != 3 || got.Histogram.FirstValid != 3 ||
		!reflect.DeepEqual(got.Line.Values, []float64{0, 0, 0.5, 0.5}) ||
		!reflect.DeepEqual(got.Signal.Values, []float64{0, 0, 0, 0.5}) ||
		!reflect.DeepEqual(got.Histogram.Values, []float64{0, 0, 0, 0}) {
		t.Fatalf("unexpected MACD channels: %+v", got)
	}
	encoded, err := json.Marshal(got.Signal)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"values":[null,null,null,0.5],"first_valid":3}` {
		t.Fatalf("unexpected MACD signal JSON: %s", encoded)
	}
	short, err := ChartMACD([]float64{1, 2}, 2, 3, 2)
	if err != nil || short.Line.FirstValid != 2 || short.Signal.FirstValid != 2 || short.Histogram.FirstValid != 2 ||
		short.Line.NullableValues()[0] != nil {
		t.Fatalf("short MACD: %+v, %v", short, err)
	}
	closes := make([]float64, 34)
	for i := range closes {
		closes[i] = float64(i + 1)
	}
	defaults, err := DefaultMACD(closes)
	if err != nil || defaults.Line.FirstValid != 25 || defaults.Signal.FirstValid != 33 || defaults.Histogram.FirstValid != 33 {
		t.Fatalf("default MACD: %+v, %v", defaults, err)
	}
	empty, err := DefaultMACD(nil)
	if err != nil || empty.Line.FirstValid != 0 || empty.Signal.FirstValid != 0 || len(empty.Histogram.Values) != 0 {
		t.Fatalf("empty MACD: %+v, %v", empty, err)
	}
}

func TestChartKDMACDRejectInvalidInputs(t *testing.T) {
	if _, err := ChartKD([]float64{1}, nil, []float64{1}, 1, 3, 3); err == nil {
		t.Fatal("accepted mismatched KD lengths")
	}
	if _, err := ChartKD([]float64{1}, []float64{0}, []float64{1}, 0, 3, 3); err == nil {
		t.Fatal("accepted zero KD window")
	}
	if _, err := ChartKD([]float64{1}, []float64{0}, []float64{1}, 1, 0, 3); err == nil {
		t.Fatal("accepted zero K smoothing")
	}
	if _, err := ChartKD([]float64{1}, []float64{0}, []float64{2}, 1, 3, 3); err == nil {
		t.Fatal("accepted KD close outside high/low")
	}
	if _, err := ChartKD([]float64{1}, []float64{0}, []float64{math.NaN()}, 1, 3, 3); err == nil {
		t.Fatal("accepted nonfinite KD close")
	}
	if _, err := ChartMACD([]float64{1}, 2, 2, 1); err == nil {
		t.Fatal("accepted equal fast and slow MACD periods")
	}
	if _, err := ChartMACD([]float64{math.Inf(1)}, 1, 2, 1); err == nil {
		t.Fatal("accepted nonfinite MACD close")
	}
}
