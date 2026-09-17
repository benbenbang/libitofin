package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestLowDiscrepancyOracles(t *testing.T) {
	s := rngSession(t)
	want := []float64{.5, .75, .25, .375, .875, .625, .125, .1875, .6875, .9375, .4375, .3125, .8125, .5625, .0625, .09375, .59375, .84375, .34375, .46875, .96875, .71875, .21875, .15625, .65625, .90625, .40625, .28125, .78125, .53125, .03125}
	for _, table := range []DirectionIntegers{Unit, Jaeckel, SobolLevitan, SobolLevitanLemieux, JoeKuoD5, JoeKuoD6, JoeKuoD7, Kuo, Kuo2, Kuo3} {
		sobol := mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 8, DirectionIntegers: &table}))
		if mustRNG(sobol.Dimension()) != 8 {
			t.Fatal("dimension")
		}
		got := mustRNG(sobol.NextSequences(len(want)))
		for row, value := range want {
			if got[row*8] != value {
				t.Fatal(table, row, got[row*8], value)
			}
		}
	}
	sobol := mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 33, Seed: 123456}))
	sums := make([]float64, 33)
	for k := 1; k <= 15; k++ {
		for i, v := range mustRNG(sobol.NextSequence()) {
			sums[i] += v
		}
		if k == 1 || k == 3 || k == 7 || k == 15 {
			for _, v := range sums {
				if !rngClose(v/float64(k), .5, 1e-15) {
					t.Fatal("homogeneity", v, k)
				}
			}
		}
	}
	halton := mustRNG(s.NewHaltonRsg(2))
	if mustRNG(halton.Dimension()) != 2 {
		t.Fatal("dimension")
	}
	for _, want := range [][]float64{{.5, 1. / 3}, {.25, 2. / 3}, {.75, 1. / 9}, {.125, 4. / 9}, {.625, 7. / 9}, {.375, 2. / 9}, {.875, 5. / 9}, {.0625, 8. / 9}} {
		got := mustRNG(halton.NextSequence())
		for i, v := range want {
			if !rngClose(got[i], v, 1e-15) {
				t.Fatal(got, want)
			}
		}
	}
	for _, r := range []randomSequence{mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 3})).randomSequence, mustRNG(s.NewHaltonRsg(3)).randomSequence} {
		if !reflect.DeepEqual(mustRNG(r.LastSequence()), []float64{0, 0, 0}) {
			t.Fatal("initial last")
		}
		if len(mustRNG(r.NextSequences(0))) != 0 {
			t.Fatal("zero batch")
		}
		values := mustRNG(r.NextSequences(7))
		if !reflect.DeepEqual(mustRNG(r.LastSequence()), values[18:21]) {
			t.Fatal("last sequence")
		}
		if _, err := r.NextSequences(int(^uint(0) >> 1)); err == nil {
			t.Fatal("overflow batch")
		}
	}
}
func TestSobolIntegerSkipAndLimits(t *testing.T) {
	s := rngSession(t)
	makeSobol := func(plain bool) *SobolRsg {
		return mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 2, PlainCounter: plain}))
	}
	raw, real := makeSobol(false), makeSobol(false)
	for range 16 {
		words, values := mustRNG(raw.NextInt32Sequence()), mustRNG(real.NextSequence())
		for i, v := range words {
			if float64(v)/4294967296 != values[i] {
				t.Fatal(words, values)
			}
		}
	}
	for _, plain := range []bool{false, true} {
		for _, drawn := range []bool{false, true} {
			r := makeSobol(plain)
			if drawn {
				mustRNG(r.NextSequence())
			}
			skipped := mustRNG(r.SkipTo(3))
			next := mustRNG(r.NextInt32Sequence())
			if (!drawn || plain) && !reflect.DeepEqual(skipped, next) {
				t.Fatal("skip should redraw", plain, drawn)
			}
			if drawn && !plain && reflect.DeepEqual(skipped, next) {
				t.Fatal("skip should advance")
			}
		}
		r := makeSobol(plain)
		if _, err := r.SkipTo(math.MaxUint32); err == nil {
			t.Fatal("invalid skip")
		}
		mustRNG(r.SkipTo(math.MaxUint32 - 1))
		mustRNG(r.NextInt32Sequence())
		if plain {
			mustRNG(r.NextInt32Sequence())
		}
		if _, err := r.NextSequence(); err == nil {
			t.Fatal("period exceeded")
		}
		mustRNG(r.SkipTo(0))
		mustRNG(r.NextSequence())
	}
	for _, d := range []int{-1, 0, 21201} {
		if _, err := s.NewSobolRsg(SobolConfig{Dimension: d}); err == nil {
			t.Fatal("dimension", d)
		}
	}
	bad := DirectionIntegers(10)
	if _, err := s.NewSobolRsg(SobolConfig{Dimension: 1, DirectionIntegers: &bad}); err == nil {
		t.Fatal("table")
	}
	if _, err := s.NewHaltonRsg(0); err == nil {
		t.Fatal("Halton dimension")
	}
}
