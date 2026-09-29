package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestVolatilityEstimators(t *testing.T) {
	close := []float64{100, 110, 99}
	yearFraction := 1.0 / 252.0
	indexed, err := SimpleLocalVolatility(close, []float64{0, yearFraction, yearFraction})
	if err != nil {
		t.Fatal(err)
	}
	constant, err := SimpleLocalVolatilityConstantFraction(close, yearFraction)
	if err != nil {
		t.Fatal(err)
	}
	if indexed.FirstValid != 1 || constant.FirstValid != 1 || indexed.Values[0] != 0 {
		t.Fatalf("unexpected local volatility warmup: %+v, %+v", indexed, constant)
	}
	for i := 1; i < len(close); i++ {
		want := math.Abs(math.Log(close[i]/close[i-1])) / math.Sqrt(yearFraction)
		if math.Abs(indexed.Values[i]-want) > 1e-12 || math.Abs(constant.Values[i]-want) > 1e-12 {
			t.Fatalf("local volatility at %d: indexed=%v constant=%v want=%v", i, indexed.Values[i], constant.Values[i], want)
		}
	}
	if indexed.NullableValues()[0] != nil || indexed.NullableValues()[1] == nil {
		t.Fatalf("unexpected nullable local volatility: %+v", indexed)
	}
	short, err := SimpleLocalVolatility([]float64{100}, []float64{0})
	if err != nil || short.FirstValid != 1 || !reflect.DeepEqual(short.Values, []float64{0}) {
		t.Fatalf("one close: %+v, %v", short, err)
	}
	empty, err := SimpleLocalVolatilityConstantFraction(nil, yearFraction)
	if err != nil || empty.FirstValid != 0 || len(empty.Values) != 0 {
		t.Fatalf("empty closes: %+v, %v", empty, err)
	}
	t.Run("constant", checkConstantVolatility)
}

func checkConstantVolatility(t *testing.T) {
	local, err := SimpleLocalVolatilityConstantFraction([]float64{100, 110, 99}, 1.0/252.0)
	if err != nil {
		t.Fatal(err)
	}
	one, err := ConstantVolatility(local, 1)
	if err != nil {
		t.Fatal(err)
	}
	if one.FirstValid != 2 || !reflect.DeepEqual(one.Values[:2], []float64{0, 0}) || math.Abs(one.Values[2]-1.0698541148988145) > 1e-12 {
		t.Fatalf("one observation window: %+v", one)
	}
	two, err := ConstantVolatility(ChartSeries{Values: []float64{1, 2, 3, 4}}, 2)
	if err != nil {
		t.Fatal(err)
	}
	if two.FirstValid != 2 || !reflect.DeepEqual(two.Values[:2], []float64{0, 0}) ||
		math.Abs(two.Values[2]-1) > 1e-12 || math.Abs(two.Values[3]-math.Sqrt(7.0/3.0)) > 1e-12 {
		t.Fatalf("two observation window: %+v", two)
	}
	signed, err := ConstantVolatility(ChartSeries{Values: []float64{-1, 2, 0}}, 2)
	if err != nil || math.Abs(signed.Values[2]-math.Sqrt(7.0/3.0)) > 1e-12 {
		t.Fatalf("signed input window: %+v, %v", signed, err)
	}
	ignoredWarmup, err := ConstantVolatility(ChartSeries{Values: []float64{math.NaN(), 1, 2}, FirstValid: 1}, 1)
	if err != nil || ignoredWarmup.FirstValid != 2 || math.Abs(ignoredWarmup.Values[2]-math.Sqrt(0.5)) > 1e-12 {
		t.Fatalf("ignored warmup: %+v, %v", ignoredWarmup, err)
	}
	short, err := ConstantVolatility(local, 2)
	if err != nil || short.FirstValid != 3 || !reflect.DeepEqual(short.Values, []float64{0, 0, 0}) {
		t.Fatalf("short input: %+v, %v", short, err)
	}
	empty, err := ConstantVolatility(ChartSeries{}, 1)
	if err != nil || empty.FirstValid != 0 || len(empty.Values) != 0 {
		t.Fatalf("empty input: %+v, %v", empty, err)
	}
}

func TestVolatilityRejectsInvalidInputs(t *testing.T) {
	if _, err := SimpleLocalVolatility([]float64{100}, nil); err == nil {
		t.Fatal("accepted mismatched close and year fraction lengths")
	}
	for _, close := range [][]float64{{0, 110}, {-1, 110}, {math.NaN(), 110}, {100, math.Inf(1)}} {
		if _, err := SimpleLocalVolatilityConstantFraction(close, 1.0/252.0); err == nil {
			t.Fatalf("accepted invalid close: %v", close)
		}
	}
	for _, fraction := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		if _, err := SimpleLocalVolatility([]float64{100, 110}, []float64{0, fraction}); err == nil {
			t.Fatalf("accepted invalid indexed year fraction: %v", fraction)
		}
		if _, err := SimpleLocalVolatilityConstantFraction(nil, fraction); err == nil {
			t.Fatalf("accepted invalid scalar year fraction for empty closes: %v", fraction)
		}
	}
	for _, firstValid := range []int{-1, 2} {
		if _, err := ConstantVolatility(ChartSeries{Values: []float64{1}, FirstValid: firstValid}, 1); err == nil {
			t.Fatalf("accepted malformed first valid index: %d", firstValid)
		}
	}
	if _, err := ConstantVolatility(ChartSeries{Values: []float64{1}}, 0); err == nil {
		t.Fatal("accepted zero window")
	}
	if _, err := ConstantVolatility(ChartSeries{Values: []float64{1}}, -1); err == nil {
		t.Fatal("accepted negative window")
	}
	for _, value := range []float64{math.NaN(), math.Inf(1)} {
		if _, err := ConstantVolatility(ChartSeries{Values: []float64{0, value}, FirstValid: 1}, 1); err == nil {
			t.Fatalf("accepted invalid volatility value: %v", value)
		}
	}
}
