package itofin

import (
	"encoding/json"
	"math"
	"reflect"
	"testing"
)

func TestChartADXIndependentFixtureAndOwnership(t *testing.T) {
	h := []float64{10, 12, 11, 14, 15, 13, 18, 16}
	l := []float64{8, 9, 7, 10, 9, 9, 15, 13}
	c := []float64{9, 11, 8, 13, 10, 12, 17, 14}
	result, err := ADX(h, l, c, 2)
	if err != nil {
		t.Fatal(err)
	}
	expected := [][]float64{
		{0, 0, 200.0 / 7, 800.0 / 19, 800.0 / 43, 32.0 / 3, 8800.0 / 171, 8800.0 / 299},
		{0, 0, 200.0 / 7, 200.0 / 19, 200.0 / 43, 8.0 / 3, 200.0 / 171, 6600.0 / 299},
		{0, 0, 0, 60, 60, 60, 860.0 / 9, 100.0 / 7},
		{0, 0, 0, 30, 45, 105.0 / 2, 2665.0 / 36, 22255.0 / 504},
	}
	for k, s := range []ChartSeries{result.PlusDI, result.MinusDI, result.DX, result.ADX} {
		valid := 2
		if k == 3 {
			valid = 3
		}
		if s.FirstValid != valid || len(s.Values) != 8 {
			t.Fatal("bad alignment")
		}
		for i, want := range expected[k] {
			if math.Abs(s.Values[i]-want) > 1e-12 {
				t.Fatalf("channel%d index%d: %g want%g", k, i, s.Values[i], want)
			}
			if (s.NullableValues()[i] == nil) != (i < valid) {
				t.Fatal("bad warmup masking")
			}
		}
	}
	h[0], l[0], c[0] = -99, -99, -99
	if math.Abs(result.PlusDI.Values[2]-200.0/7) > 1e-12 {
		t.Fatal("result aliases input")
	}
	result.PlusDI.Values[2] = -99
	if math.Abs(result.MinusDI.Values[2]-200.0/7) > 1e-12 {
		t.Fatal("channels alias")
	}
	fresh, err := ADX([]float64{10, 12, 11}, []float64{8, 9, 7}, []float64{9, 11, 8}, 2)
	if err != nil || math.Abs(fresh.PlusDI.Values[2]-200.0/7) > 1e-12 {
		t.Fatal("calls share outputs")
	}
}

func TestChartADXDefaultShortPeriodOneFlatAndDirections(t *testing.T) {
	c := make([]float64, 30)
	for i := range c {
		c[i] = float64(i)
	}
	def, err := DefaultADX(c, c, c)
	explicit, e := ADX(c, c, c, 14)
	if err != nil || e != nil || !reflect.DeepEqual(def, explicit) || def.DX.FirstValid != 14 || def.ADX.FirstValid != 27 {
		t.Fatal("bad defaults")
	}
	for _, p := range []int{1, 2, 14, math.MaxInt} {
		for n := 0; n < 5; n++ {
			r, err := ADX(c[:n], c[:n], c[:n], p)
			if err != nil {
				t.Fatal(err)
			}
			valid, adxValid := p, n
			if valid > n {
				valid = n
			}
			if p <= n {
				adxValid = 2*p - 1
				if adxValid > n {
					adxValid = n
				}
			}
			if r.DX.FirstValid != valid || r.ADX.FirstValid != adxValid {
				t.Fatal("bad short validity")
			}
		}
	}
	r, err := ADX([]float64{1, 2}, []float64{1, 2}, []float64{1, 2}, 1)
	if err != nil || !reflect.DeepEqual(r.DX.Values, []float64{0, 100}) || !reflect.DeepEqual(r.DX, r.ADX) {
		t.Fatal("bad period one")
	}
	encoded, err := json.Marshal(r.ADX)
	if err != nil || string(encoded) != `{"values":[null,100],"first_valid":1}` {
		t.Fatalf("bad JSON: %s", encoded)
	}
	for _, sign := range []float64{1, -1} {
		for i := range c {
			c[i] = sign * float64(i)
		}
		r, err := ADX(c, c, c, 2)
		if err != nil || r.ADX.Values[3] != 100 || r.DX.Values[2] != 100 {
			t.Fatal("bad direction")
		}
	}
	flat := []float64{-3, -3, -3, -3}
	r, err = ADX(flat, flat, flat, 2)
	if err != nil || r.DX.Values[3] != 0 || r.ADX.Values[3] != 0 {
		t.Fatal("bad flat")
	}
}

func TestChartADXRejectInvalidAndWarmupOverflow(t *testing.T) {
	for _, p := range []int{0, -1} {
		if _, err := ADX(nil, nil, nil, p); err == nil {
			t.Fatal("accepted period")
		}
	}
	for _, input := range []struct{ h, l, c []float64 }{
		{nil, []float64{0}, []float64{1}}, {[]float64{2}, nil, []float64{1}}, {[]float64{2}, []float64{0}, nil},
		{[]float64{math.NaN()}, []float64{0}, []float64{1}}, {[]float64{2}, []float64{math.Inf(-1)}, []float64{1}}, {[]float64{2}, []float64{0}, []float64{math.Inf(1)}},
		{[]float64{2}, []float64{3}, []float64{1}}, {[]float64{math.MaxFloat64}, []float64{-math.MaxFloat64}, []float64{0}},
		{[]float64{math.MaxFloat64, -math.MaxFloat64}, []float64{0, -math.MaxFloat64}, []float64{0, -math.MaxFloat64}},
	} {
		if got, err := ADX(input.h, input.l, input.c, 14); err == nil || got.PlusDI.Values != nil {
			t.Fatal("accepted invalid input")
		}
	}
	large := make([]float64, DefaultMaxOutputValues/4+1)
	if _, err := ADX(large, large, large, 14); err == nil {
		t.Fatal("accepted output limit")
	}
	max, err := ADX([]float64{0, math.MaxFloat64, math.MaxFloat64}, []float64{0, 0, 0}, []float64{0, 0, 0}, 2)
	if err != nil || math.IsInf(max.PlusDI.Values[2], 0) {
		t.Fatal("avoidable multiply/seed overflow")
	}
}
