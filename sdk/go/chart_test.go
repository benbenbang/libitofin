package itofin

import (
	"encoding/json"
	"math"
	"reflect"
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
