package itofin

import (
	"encoding/csv"
	"encoding/json"
	"math"
	"os"
	"reflect"
	"strconv"
	"testing"
)

func TestChartWilliamsRIndependentFixtureAndOwnership(t *testing.T) {
	file, err := os.Open("../../crates/libitofin/tests/data/chart/williams_r.csv")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	rows, err := csv.NewReader(file).ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	columns := make([][]float64, 4)
	for _, row := range rows[1:] {
		for j, value := range row {
			number, err := strconv.ParseFloat(value, 64)
			if err != nil {
				t.Fatal(err)
			}
			columns[j] = append(columns[j], number)
		}
	}
	result, err := WilliamsR(columns[0], columns[1], columns[2], 3)
	if err != nil || result.FirstValid != 2 || len(result.Values) != 9 {
		t.Fatalf("unexpected Williams R: %+v %v", result, err)
	}
	for i, want := range columns[3] {
		if math.Abs(result.Values[i]-want) > 1e-12 {
			t.Fatalf("Williams R[%d]=%g want %g", i, result.Values[i], want)
		}
	}
	nullable := result.NullableValues()
	if nullable[0] != nil || nullable[1] != nil || nullable[6] == nil || *nullable[6] != 0 {
		t.Fatal("warmup versus valid zero lost")
	}
	columns[0][0] = -99
	if math.Abs(result.Values[2]+600.0/7) > 1e-12 {
		t.Fatal("input ownership leaked")
	}
	result.Values[2] = 123
	fresh, err := WilliamsR([]float64{12, 16, 11}, []float64{10, 14, 9}, []float64{11, 15, 10}, 3)
	if err != nil || math.Abs(fresh.Values[2]+600.0/7) > 1e-12 {
		t.Fatal("output ownership leaked")
	}
}

func TestChartWilliamsRDefaultWarmupBoundsAndFlat(t *testing.T) {
	empty, err := DefaultWilliamsR(nil, nil, nil)
	if err != nil || empty.FirstValid != 0 || len(empty.Values) != 0 {
		t.Fatal(empty, err)
	}
	short, err := WilliamsR([]float64{2}, []float64{0}, []float64{1}, 3)
	if err != nil || short.FirstValid != 1 {
		t.Fatal(short, err)
	}
	encoded, err := json.Marshal(short)
	if err != nil || string(encoded) != `{"values":[null],"first_valid":1}` {
		t.Fatal(string(encoded), err)
	}
	flat := make([]float64, 15)
	result, err := DefaultWilliamsR(flat, flat, flat)
	explicit, err2 := WilliamsR(flat, flat, flat, 14)
	if err != nil || err2 != nil || result.FirstValid != 13 || !reflect.DeepEqual(result, explicit) || result.Values[13] != -50 {
		t.Fatal(result, err, err2)
	}
	for _, input := range []struct {
		high, low, close, want []float64
		period                 int
	}{
		{[]float64{2, 2}, []float64{0, 0}, []float64{2, 0}, []float64{0, -100}, 1},
		{[]float64{-3, -3, -3}, []float64{-3, -3, -3}, []float64{-3, -3, -3}, []float64{0, -50, -50}, 2},
		{[]float64{1, 2, 3}, []float64{1, 2, 3}, []float64{1, 2, 3}, []float64{0, 0, 0}, 2},
		{[]float64{3, 2, 1}, []float64{3, 2, 1}, []float64{3, 2, 1}, []float64{0, -100, -100}, 2},
		{[]float64{math.SmallestNonzeroFloat64, math.MaxFloat64}, []float64{0, 0}, []float64{0, 0}, []float64{-100, -100}, 1},
	} {
		got, err := WilliamsR(input.high, input.low, input.close, input.period)
		if err != nil || !reflect.DeepEqual(got.Values, input.want) {
			t.Fatal(got, err)
		}
	}
}

func TestChartWilliamsRRejectInvalidAndWarmupOverflow(t *testing.T) {
	for _, values := range []struct{ high, low, close []float64 }{
		{nil, []float64{0}, []float64{1}},
		{[]float64{2}, nil, []float64{1}},
		{[]float64{2}, []float64{0}, nil},
		{[]float64{math.NaN()}, []float64{0}, []float64{1}},
		{[]float64{2}, []float64{math.Inf(-1)}, []float64{1}},
		{[]float64{2}, []float64{0}, []float64{math.Inf(1)}},
		{[]float64{2}, []float64{3}, []float64{1}},
		{[]float64{2}, []float64{0}, []float64{3}},
		{[]float64{math.MaxFloat64}, []float64{-math.MaxFloat64}, []float64{0}},
		{[]float64{-math.MaxFloat64, math.MaxFloat64}, []float64{-math.MaxFloat64, math.MaxFloat64}, []float64{-math.MaxFloat64, math.MaxFloat64}},
	} {
		got, err := DefaultWilliamsR(values.high, values.low, values.close)
		if err == nil || got.Values != nil {
			t.Fatal("accepted invalid HLC or overflow")
		}
	}
	for _, period := range []int{0, -1} {
		got, err := WilliamsR(nil, nil, nil, period)
		if err == nil || got.Values != nil {
			t.Fatal("accepted invalid period")
		}
	}
}
