package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestPseudoRandomSequences(t *testing.T) {
	s := rngSession(t)
	scalar := mustRNG(s.NewUniformRandomGenerator(42))
	mustRNG(scalar.NextReals(5))
	uniform := mustRNG(s.NewUniformRandomSequenceGenerator(3, scalar))
	gaussian := mustRNG(s.NewGaussianRandomSequenceGenerator(uniform))
	if mustRNG(uniform.Dimension()) != 3 || mustRNG(gaussian.Dimension()) != 3 {
		t.Fatal("dimension")
	}
	if !reflect.DeepEqual(mustRNG(uniform.LastSequence()), []float64{0, 0, 0}) || !reflect.DeepEqual(mustRNG(gaussian.LastSequence()), []float64{0, 0, 0}) {
		t.Fatal("initial last")
	}
	for range 20 {
		u, g := mustRNG(uniform.NextSequence()), mustRNG(gaussian.NextSequence())
		if !reflect.DeepEqual(u, mustRNG(scalar.NextReals(3))) {
			t.Fatal("source state was not copied")
		}
		for i, z := range g {
			if !rngClose(.5*math.Erfc(-z/math.Sqrt2), u[i], 1e-8) {
				t.Fatal(z, u[i])
			}
		}
		if !reflect.DeepEqual(u, mustRNG(uniform.LastSequence())) || !reflect.DeepEqual(g, mustRNG(gaussian.LastSequence())) {
			t.Fatal("last advanced")
		}
	}
	for _, gaussian := range []bool{false, true} {
		var single, batch randomSequence
		if gaussian {
			single = mustRNG(s.GaussianRandomSequenceGeneratorWithSeed(4, 42)).randomSequence
			batch = mustRNG(s.NewGaussianRandomSequenceGenerator(mustRNG(s.UniformRandomSequenceGeneratorWithSeed(4, 42)))).randomSequence
		} else {
			single = mustRNG(s.UniformRandomSequenceGeneratorWithSeed(4, 42)).randomSequence
			batch = mustRNG(s.NewUniformRandomSequenceGenerator(4, mustRNG(s.NewUniformRandomGenerator(42)))).randomSequence
		}
		values := mustRNG(batch.NextSequences(10))
		for row := range 10 {
			if !reflect.DeepEqual(values[row*4:(row+1)*4], mustRNG(single.NextSequence())) {
				t.Fatal("batch layout")
			}
		}
		if got := mustRNG(single.NextSequences(0)); len(got) != 0 {
			t.Fatal(got)
		}
		if _, err := single.NextSequences(-1); err == nil {
			t.Fatal("negative count")
		}
		if _, err := single.NextSequences(int(^uint(0) >> 1)); err == nil {
			t.Fatal("overflow count")
		}
		values[0] = 99
		if mustRNG(batch.LastSequence())[0] == 99 {
			t.Fatal("returned slice aliases state")
		}
	}
	scalar.Close()
	uniform.Close()
	mustRNG(gaussian.NextSequence())
}
func TestPseudoSequenceValidation(t *testing.T) {
	s, other := rngSession(t), rngSession(t)
	source := mustRNG(s.NewUniformRandomGenerator(42))
	for _, d := range []int{0, -1, DefaultMaxOutputValues + 1} {
		if _, err := s.NewUniformRandomSequenceGenerator(d, source); err == nil {
			t.Fatal("invalid dimension", d)
		}
		if _, err := s.GaussianRandomSequenceGeneratorWithSeed(d, 42); err == nil {
			t.Fatal("invalid dimension", d)
		}
	}
	if _, err := s.NewUniformRandomSequenceGenerator(3, nil); err == nil {
		t.Fatal("nil scalar")
	}
	if _, err := s.NewGaussianRandomSequenceGenerator(nil); err == nil {
		t.Fatal("nil sequence")
	}
	if _, err := other.NewUniformRandomSequenceGenerator(3, source); err != ErrSessionMismatch {
		t.Fatal(err)
	}
	uniform := mustRNG(s.UniformRandomSequenceGeneratorWithSeed(3, 42))
	if _, err := other.NewGaussianRandomSequenceGenerator(uniform); err != ErrSessionMismatch {
		t.Fatal(err)
	}
	uniform.Close()
	if _, err := s.NewGaussianRandomSequenceGenerator(uniform); err == nil {
		t.Fatal("closed source")
	}
	if _, err := uniform.Dimension(); err == nil {
		t.Fatal("closed dimension")
	}
}
