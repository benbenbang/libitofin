package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type PoissonRandomGenerator struct{ object }
type PoissonRandomSequenceGenerator struct {
	object
	dimension int
}

func (s *Session) poissonGenerator(dimension int, seed uint32, lambda float64) (object, error) {
	if dimension <= 0 || dimension > DefaultMaxOutputValues {
		return object{}, fmt.Errorf("itofin: dimension outside [1, %d]", DefaultMaxOutputValues)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_poisson_rng_new(s.ctx, C.size_t(dimension), C.uint32_t(seed), C.double(lambda), &id, &e), &e)
	})
	return object{s, uint64(id)}, err
}

// NewPoissonRandomGenerator constructs MT19937 Poisson draws; seed zero is random.
func (s *Session) NewPoissonRandomGenerator(seed uint32, lambda float64) (*PoissonRandomGenerator, error) {
	o, err := s.poissonGenerator(1, seed, lambda)
	if err != nil {
		return nil, err
	}
	return &PoissonRandomGenerator{o}, nil
}

// NewPoissonRandomSequenceGenerator constructs independent fixed-dimensional draws.
func (s *Session) NewPoissonRandomSequenceGenerator(dimension int, seed uint32, lambda float64) (*PoissonRandomSequenceGenerator, error) {
	o, err := s.poissonGenerator(dimension, seed, lambda)
	if err != nil {
		return nil, err
	}
	return &PoissonRandomSequenceGenerator{o, dimension}, nil
}

func poissonDraw(o object, dimension int, last bool) ([]float64, error) {
	out, err := rngBuffer(dimension)
	if err != nil {
		return nil, err
	}
	var flag C.int32_t
	if last {
		flag = 1
	}
	err = o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_poisson_rng_draw(o.session.ctx, C.uint64_t(o.id), flag, doubles(out), C.size_t(len(out)), &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return out, nil
}

func poissonCopy(o object) (object, error) {
	var id C.uint64_t
	err := o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_poisson_rng_copy(o.session.ctx, C.uint64_t(o.id), &id, &e), &e)
	})
	return object{o.session, uint64(id)}, err
}

func (r *PoissonRandomGenerator) NextReal() (float64, error) {
	out, err := poissonDraw(r.object, 1, false)
	if err != nil {
		return 0, err
	}
	return out[0], nil
}
func (r *PoissonRandomGenerator) Copy() (*PoissonRandomGenerator, error) {
	o, err := poissonCopy(r.object)
	if err != nil {
		return nil, err
	}
	return &PoissonRandomGenerator{o}, nil
}
func (r *PoissonRandomSequenceGenerator) Dimension() (int, error) {
	if _, err := r.LastSequence(); err != nil {
		return 0, err
	}
	return r.dimension, nil
}
func (r *PoissonRandomSequenceGenerator) NextSequence() ([]float64, error) {
	return poissonDraw(r.object, r.dimension, false)
}
func (r *PoissonRandomSequenceGenerator) LastSequence() ([]float64, error) {
	return poissonDraw(r.object, r.dimension, true)
}
func (r *PoissonRandomSequenceGenerator) Copy() (*PoissonRandomSequenceGenerator, error) {
	o, err := poissonCopy(r.object)
	if err != nil {
		return nil, err
	}
	return &PoissonRandomSequenceGenerator{o, r.dimension}, nil
}
