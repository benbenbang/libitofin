package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type UniformRandomGenerator struct{ object }
type GaussianRandomGenerator struct{ object }

func words(values []uint32) *C.uint32_t {
	if len(values) == 0 {
		return nil
	}
	return (*C.uint32_t)(unsafe.Pointer(&values[0]))
}

func rngBuffer(count int) ([]float64, error) {
	if count < 0 || count > DefaultMaxOutputValues {
		return nil, fmt.Errorf("itofin: RNG output exceeds allocation limit")
	}
	return make([]float64, count), nil
}

// NewUniformRandomGenerator uses MT19937; seed zero selects a random seed.
func (s *Session) NewUniformRandomGenerator(seed uint32) (*UniformRandomGenerator, error) {
	return s.uniformGenerator(seed, nil, false)
}
func (s *Session) UniformRandomGeneratorFromSeeds(seeds []uint32) (*UniformRandomGenerator, error) {
	return s.uniformGenerator(0, seeds, true)
}
func (s *Session) uniformGenerator(seed uint32, seeds []uint32, fromSeeds bool) (*UniformRandomGenerator, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_uniform_rng_new(s.ctx, C.uint32_t(seed), words(seeds), C.size_t(len(seeds)), C.bool(fromSeeds), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &UniformRandomGenerator{object{s, uint64(id)}}, nil
}

// NewGaussianRandomGenerator copies the current uniform state for Box-Muller draws.
func (s *Session) NewGaussianRandomGenerator(source *UniformRandomGenerator) (*GaussianRandomGenerator, error) {
	if source == nil {
		return nil, errNilArgument("source")
	}
	if err := sameSession(s, source.object); err != nil {
		return nil, err
	}
	return s.gaussianGenerator(source.id, 0)
}
func (s *Session) GaussianRandomGeneratorWithSeed(seed uint32) (*GaussianRandomGenerator, error) {
	return s.gaussianGenerator(0, seed)
}
func (s *Session) gaussianGenerator(source uint64, seed uint32) (*GaussianRandomGenerator, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gaussian_rng_new(s.ctx, C.uint64_t(source), C.uint32_t(seed), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GaussianRandomGenerator{object{s, uint64(id)}}, nil
}
func scalarDraw(o object, gaussian bool, count int) ([]float64, error) {
	out, err := rngBuffer(count)
	if err != nil {
		return nil, err
	}
	err = o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_rng_draw(o.session.ctx, C.uint64_t(o.id), C.bool(gaussian), C.size_t(count), doubles(out), C.size_t(len(out)), &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return out, nil
}
func (r UniformRandomGenerator) NextReals(count int) ([]float64, error) {
	return scalarDraw(r.object, false, count)
}
func (r UniformRandomGenerator) NextReal() (float64, error) {
	out, err := r.NextReals(1)
	if err != nil {
		return 0, err
	}
	return out[0], nil
}
func (r UniformRandomGenerator) NextU32() (uint32, error) {
	var out C.uint32_t
	err := r.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_uniform_rng_u32(r.session.ctx, C.uint64_t(r.id), &out, &e), &e)
	})
	return uint32(out), err
}
func (r GaussianRandomGenerator) NextGaussians(count int) ([]float64, error) {
	return scalarDraw(r.object, true, count)
}
func (r GaussianRandomGenerator) NextGaussian() (float64, error) {
	out, err := r.NextGaussians(1)
	if err != nil {
		return 0, err
	}
	return out[0], nil
}
