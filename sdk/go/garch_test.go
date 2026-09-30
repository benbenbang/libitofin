package itofin

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"math"
	"os"
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

func TestGarch11FitSyntheticReturns(t *testing.T) {
	returns := make([]float64, 64)
	for i := range returns {
		returns[i] = float64((i*37)%53-26) / 1000
	}
	wantReturns := append([]float64(nil), returns...)
	result, err := Garch11Fit(returns)
	if err != nil {
		t.Fatal(err)
	}
	if result.Alpha < 0 || result.Beta < 0 || result.Omega <= 0 || result.Alpha+result.Beta >= 1 ||
		!isFiniteGarch(result.Alpha) || !isFiniteGarch(result.Beta) || !isFiniteGarch(result.Omega) ||
		!isFiniteGarch(result.LogLikelihood) || !isFiniteGarch(result.NextVariance) || result.NextVariance <= 0 {
		t.Fatalf("invalid fitted result: %+v", result)
	}
	if !reflect.DeepEqual(returns, wantReturns) {
		t.Fatalf("fit changed returns: %v", returns)
	}
	filtered, err := Garch11Filter(returns, result.Alpha, result.Beta, result.Omega/(1-result.Alpha-result.Beta))
	if err != nil {
		t.Fatal(err)
	}
	if math.Abs(filtered.NextVariance-result.NextVariance) > 1e-12 {
		t.Fatalf("fit forecast %.16g differs from filter %.16g", result.NextVariance, filtered.NextVariance)
	}
	encoded, err := json.Marshal(result)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(encoded, &fields); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"alpha", "beta", "omega", "log_likelihood", "next_variance"} {
		if len(fields[name]) == 0 {
			t.Fatalf("missing %s: %s", name, encoded)
		}
	}
}

func TestGarch11FitQuantLibFixture(t *testing.T) {
	const fixtureDir = "../../crates/libitofin/tests/fixtures/garch_fit/"
	metadata, err := os.ReadFile(fixtureDir + "oracle.json")
	if err != nil {
		t.Fatal(err)
	}
	var oracle struct {
		Input struct {
			Count                  int    `json:"count"`
			ReturnsSHA256Float64LE string `json:"returns_sha256_f64_le"`
		} `json:"input"`
		Expected Garch11FitResult `json:"expected"`
	}
	if err := json.Unmarshal(metadata, &oracle); err != nil {
		t.Fatal(err)
	}
	encodedReturns, err := os.ReadFile(fixtureDir + "returns.bin")
	if err != nil {
		t.Fatal(err)
	}
	if len(encodedReturns) != oracle.Input.Count*8 {
		t.Fatalf("fixture has %d bytes for %d returns", len(encodedReturns), oracle.Input.Count)
	}
	if digest := fmt.Sprintf("%x", sha256.Sum256(encodedReturns)); digest != oracle.Input.ReturnsSHA256Float64LE {
		t.Fatalf("fixture digest %s differs from %s", digest, oracle.Input.ReturnsSHA256Float64LE)
	}
	returns := make([]float64, oracle.Input.Count)
	for i := range returns {
		returns[i] = math.Float64frombits(binary.LittleEndian.Uint64(encodedReturns[i*8:]))
	}
	result, err := Garch11Fit(returns)
	if err != nil {
		t.Fatal(err)
	}
	for _, field := range []struct {
		name string
		got  float64
		want float64
	}{
		{"alpha", result.Alpha, oracle.Expected.Alpha},
		{"beta", result.Beta, oracle.Expected.Beta},
		{"omega", result.Omega, oracle.Expected.Omega},
		{"log_likelihood", result.LogLikelihood, oracle.Expected.LogLikelihood},
		{"next_variance", result.NextVariance, oracle.Expected.NextVariance},
	} {
		if !isFiniteGarch(field.got) || math.Abs(field.got-field.want) > 1e-6 {
			t.Errorf("%s: got %.16g, want %.16g", field.name, field.got, field.want)
		}
	}
}

func isFiniteGarch(value float64) bool {
	return !math.IsNaN(value) && !math.IsInf(value, 0)
}

func TestGarch11FitRejectsInvalidReturns(t *testing.T) {
	for _, returns := range [][]float64{
		nil, {}, {0.1}, {0.1, 0.2}, {0.1, 0.2, 0.3},
		{0.1, -0.1, 0.1, -0.1},
		{0.1, math.NaN(), 0.2},
		{0.1, math.Inf(1), 0.2},
		{0.1, math.Inf(-1), 0.2},
		{0.1, math.MaxFloat64, 0.2},
	} {
		if result, err := Garch11Fit(returns); err == nil || result != (Garch11FitResult{}) {
			t.Fatalf("accepted invalid returns %v: result %+v, error %v", returns, result, err)
		}
	}
	if _, err := Garch11Fit(make([]float64, 100_001)); err == nil {
		t.Fatal("accepted too many returns")
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
