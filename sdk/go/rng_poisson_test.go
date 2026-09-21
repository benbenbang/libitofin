package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestPoissonQuantLibSeedOraclesAndCopies(t *testing.T) {
	s := mustRNG(NewSession())
	defer s.Close()
	for _, c := range []struct{ rate, expected float64 }{{1, 108}, {4, 409}} {
		scalar := mustRNG(s.NewPoissonRandomGenerator(1234, c.rate))
		sequence := mustRNG(s.NewPoissonRandomSequenceGenerator(100, 1234, c.rate))
		if mustRNG(sequence.Dimension()) != 100 {
			t.Fatal("dimension")
		}
		if !reflect.DeepEqual(mustRNG(sequence.LastSequence()), make([]float64, 100)) {
			t.Fatal("initial last")
		}
		values := mustRNG(sequence.NextSequence())
		sum := 0.0
		for _, value := range values {
			if math.IsNaN(value) || math.IsInf(value, 0) || value < 0 || math.Trunc(value) != value {
				t.Fatal("invalid count")
			}
			if value != mustRNG(scalar.NextReal()) {
				t.Fatal("scalar/sequence mismatch")
			}
			sum += value
		}
		if sum != c.expected {
			t.Fatalf("sum %g want %g", sum, c.expected)
		}
		if !reflect.DeepEqual(values, mustRNG(sequence.LastSequence())) {
			t.Fatal("last")
		}
		clone := mustRNG(sequence.Copy())
		if !reflect.DeepEqual(mustRNG(clone.NextSequence()), mustRNG(sequence.NextSequence())) {
			t.Fatal("copy state")
		}
		scalarCopy := mustRNG(scalar.Copy())
		if mustRNG(scalarCopy.NextReal()) != mustRNG(scalar.NextReal()) {
			t.Fatal("scalar copy")
		}
		if err := scalar.Close(); err != nil {
			t.Fatal(err)
		}
		if err := sequence.Close(); err != nil {
			t.Fatal(err)
		}
		if _, err := scalarCopy.NextReal(); err != nil {
			t.Fatal(err)
		}
		if _, err := clone.NextSequence(); err != nil {
			t.Fatal(err)
		}
	}
}

func TestPoissonInvalidInputsAndClosedState(t *testing.T) {
	s := mustRNG(NewSession())
	defer s.Close()
	for _, rate := range []float64{0, -1, math.NaN(), math.Inf(1), 750} {
		if _, err := s.NewPoissonRandomGenerator(42, rate); err == nil {
			t.Fatal("invalid scalar rate")
		}
		if _, err := s.NewPoissonRandomSequenceGenerator(3, 42, rate); err == nil {
			t.Fatal("invalid sequence rate")
		}
	}
	for _, dimension := range []int{0, -1, DefaultMaxOutputValues + 1} {
		if _, err := s.NewPoissonRandomSequenceGenerator(dimension, 42, 1); err == nil {
			t.Fatal("invalid dimension")
		}
	}
	sequence := mustRNG(s.NewPoissonRandomSequenceGenerator(3, 42, 1))
	if err := sequence.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := sequence.Copy(); err == nil {
		t.Fatal("copy closed")
	}
	if _, err := sequence.Dimension(); err == nil {
		t.Fatal("dimension closed")
	}
	if _, err := sequence.NextSequence(); err == nil {
		t.Fatal("draw closed")
	}
	if _, err := s.NewPoissonRandomGenerator(42, 1); err != nil {
		t.Fatal(err)
	}
}
