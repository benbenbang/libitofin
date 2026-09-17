package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type randomSequence struct{ object }
type UniformRandomSequenceGenerator struct{ randomSequence }
type GaussianRandomSequenceGenerator struct{ randomSequence }

func (s *Session) NewUniformRandomSequenceGenerator(dimension int, source *UniformRandomGenerator) (*UniformRandomSequenceGenerator, error) {
	if source == nil {
		return nil, errNilArgument("source")
	}
	if err := sameSession(s, source.object); err != nil {
		return nil, err
	}
	r, err := s.randomSequence(source.id, dimension, 0, false)
	if err != nil {
		return nil, err
	}
	return &UniformRandomSequenceGenerator{r}, nil
}
func (s *Session) UniformRandomSequenceGeneratorWithSeed(dimension int, seed uint32) (*UniformRandomSequenceGenerator, error) {
	r, err := s.randomSequence(0, dimension, seed, false)
	if err != nil {
		return nil, err
	}
	return &UniformRandomSequenceGenerator{r}, nil
}
func (s *Session) NewGaussianRandomSequenceGenerator(source *UniformRandomSequenceGenerator) (*GaussianRandomSequenceGenerator, error) {
	if source == nil {
		return nil, errNilArgument("source")
	}
	if err := sameSession(s, source.object); err != nil {
		return nil, err
	}
	r, err := s.randomSequence(source.id, 1, 0, true)
	if err != nil {
		return nil, err
	}
	return &GaussianRandomSequenceGenerator{r}, nil
}
func (s *Session) GaussianRandomSequenceGeneratorWithSeed(dimension int, seed uint32) (*GaussianRandomSequenceGenerator, error) {
	r, err := s.randomSequence(0, dimension, seed, true)
	if err != nil {
		return nil, err
	}
	return &GaussianRandomSequenceGenerator{r}, nil
}
func (s *Session) randomSequence(source uint64, dimension int, seed uint32, gaussian bool) (randomSequence, error) {
	var id C.uint64_t
	if dimension <= 0 {
		return randomSequence{}, fmt.Errorf("itofin: dimension must be positive")
	}
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_random_sequence_new(s.ctx, C.uint64_t(source), C.size_t(dimension), C.uint32_t(seed), C.bool(gaussian), &id, &e), &e)
	})
	return randomSequence{object{s, uint64(id)}}, err
}
func (r randomSequence) Dimension() (int, error) {
	var out C.size_t
	err := r.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_rng_sequence_dimension(r.session.ctx, C.uint64_t(r.id), &out, &e), &e)
	})
	return int(out), err
}
func (r randomSequence) NextSequence() ([]float64, error) { return r.draw(1, false) }
func (r randomSequence) LastSequence() ([]float64, error) { return r.draw(1, true) }

// NextSequences returns count rows in row-major order, bounded by DefaultMaxOutputValues.
func (r randomSequence) NextSequences(count int) ([]float64, error) { return r.draw(count, false) }
func (r randomSequence) draw(count int, last bool) ([]float64, error) {
	var out []float64
	err := r.session.invoke(func() error {
		var e C.ItofinError
		var dimension C.size_t
		if err := ffiError(C.itofin_rng_sequence_dimension(r.session.ctx, C.uint64_t(r.id), &dimension, &e), &e); err != nil {
			return err
		}
		size, err := checkedProduct(count, int(dimension))
		if err != nil {
			return err
		}
		out, err = rngBuffer(size)
		if err != nil {
			return err
		}
		return ffiError(C.itofin_rng_sequence_draw(r.session.ctx, C.uint64_t(r.id), C.size_t(count), C.bool(last), doubles(out), C.size_t(len(out)), &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return out, nil
}
