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
