package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"testing"
)

func TestGarch11FilterQuantLibFixture(t *testing.T) {
	returns := []float64{0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1}
	result, err := Garch11Filter(returns, 0.2, 0.3, 0.4)
	if err != nil {
		t.Fatal(err)
	}
	if result.ConditionalVolatility.FirstValid != 1 || len(result.ConditionalVolatility.Values) != len(returns) || result.ConditionalVolatility.Values[0] != 0 {
		t.Fatalf("unexpected warmup or alignment: %+v", result)
	}
	want := []float64{0.452769, 0.513323, 0.530141, 0.5350841, 0.536558, 0.536999, 0.537132, 0.537171, 0.537183}
	for i, expected := range want {
		if got := result.ConditionalVolatility.Values[i+1]; math.Abs(got-expected) > 1e-6 {
			t.Fatalf("volatility at %d: got %.16g, want %.16g", i+1, got, expected)
		}
	}
	if math.Abs(result.NextVariance-0.288569783635) > 1e-12 {
		t.Fatalf("next variance: got %.16g", result.NextVariance)
	}
	if !reflect.DeepEqual(returns, []float64{0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1}) {
		t.Fatalf("returns changed: %v", returns)
	}
	if result.ConditionalVolatility.NullableValues()[0] != nil {
		t.Fatal("warmup is not nullable")
	}
	encoded, err := json.Marshal(result)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(encoded, &fields); err != nil {
		t.Fatal(err)
	}
	if len(fields["conditional_volatility"]) == 0 || len(fields["next_variance"]) == 0 {
		t.Fatalf("missing GARCH result fields: %s", encoded)
	}
}

func TestGarch11FilterOneReturnAndForecast(t *testing.T) {
	result, err := Garch11Filter([]float64{0.1}, 0.2, 0.3, 0.4)
	if err != nil {
		t.Fatal(err)
	}
	if result.ConditionalVolatility.FirstValid != 1 || !reflect.DeepEqual(result.ConditionalVolatility.Values, []float64{0}) {
		t.Fatalf("one-return alignment: %+v", result)
	}
	if math.Abs(result.NextVariance-0.205) > 1e-12 {
		t.Fatalf("one-return forecast: %.16g", result.NextVariance)
	}
	next, err := Garch11Forecast(0.1, 0.01, 0.2, 0.3, 0.4)
	if err != nil {
		t.Fatal(err)
	}
	if math.Abs(next-result.NextVariance) > 1e-12 {
		t.Fatalf("scalar forecast %.16g differs from filter %.16g", next, result.NextVariance)
	}
}

func TestGarch11FilterUsesPrecedingReturn(t *testing.T) {
	result, err := Garch11Filter([]float64{0.1, 0.2, 0.3}, 0.2, 0.3, 0.4)
	if err != nil {
		t.Fatal(err)
	}
	if math.Abs(result.ConditionalVolatility.Values[1]-math.Sqrt(0.205)) > 1e-14 ||
		math.Abs(result.ConditionalVolatility.Values[2]-math.Sqrt(0.2695)) > 1e-14 ||
		math.Abs(result.NextVariance-0.29885) > 1e-14 {
		t.Fatalf("GARCH return alignment: %+v", result)
	}
}

func TestGarch11RejectsInvalidInputs(t *testing.T) {
	if _, err := Garch11Filter(nil, 0.2, 0.3, 0.4); err == nil {
		t.Fatal("accepted empty returns")
	}
	for _, returns := range [][]float64{{math.NaN(), 0.1}, {math.Inf(1), 0.1}, {0.1, math.Inf(-1)}, {math.MaxFloat64, 0.1}} {
		if _, err := Garch11Filter(returns, 0.2, 0.3, 0.4); err == nil {
			t.Fatalf("accepted invalid returns: %v", returns)
		}
	}
	for _, params := range [][3]float64{
		{-0.1, 0.3, 0.4}, {0.2, -0.3, 0.4}, {0.2, 0.3, -0.4},
		{0.7, 0.3, 0.4}, {math.NaN(), 0.3, 0.4}, {0.2, math.Inf(1), 0.4},
		{0.2, 0.3, math.NaN()},
	} {
		if _, err := Garch11Filter([]float64{0.1, 0.2}, params[0], params[1], params[2]); err == nil {
			t.Fatalf("filter accepted invalid parameters: %v", params)
		}
		if _, err := Garch11Forecast(0.1, 0.01, params[0], params[1], params[2]); err == nil {
			t.Fatalf("forecast accepted invalid parameters: %v", params)
		}
	}
	for _, pair := range [][2]float64{
		{math.NaN(), 0.01}, {math.Inf(1), 0.01}, {0.1, -0.01},
		{0.1, math.Inf(1)}, {math.MaxFloat64, 0.01},
	} {
		if _, err := Garch11Forecast(pair[0], pair[1], 0.2, 0.3, 0.4); err == nil {
			t.Fatalf("forecast accepted invalid state: %v", pair)
		}
	}
	tooLong := make([]float64, DefaultMaxOutputValues+1)
	if _, err := Garch11Filter(tooLong, 0.2, 0.3, 0.4); err == nil {
		t.Fatal("accepted output above limit")
	}
}
