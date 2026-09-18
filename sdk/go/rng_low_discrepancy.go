package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type DirectionIntegers int32

const (
	Unit DirectionIntegers = iota
	Jaeckel
	SobolLevitan
	SobolLevitanLemieux
	JoeKuoD5
	JoeKuoD6
	JoeKuoD7
	Kuo
	Kuo2
	Kuo3
)

type SobolRsg struct{ randomSequence }
type HaltonRsg struct{ randomSequence }

// SobolConfig uses a literal seed even when zero. Nil DirectionIntegers selects
// Jaeckel; PlainCounter switches from QuantLib's default Gray-code counter.
type SobolConfig struct {
	Dimension         int
	Seed              uint64
	DirectionIntegers *DirectionIntegers
	PlainCounter      bool
}

func (s *Session) NewSobolRsg(c SobolConfig) (*SobolRsg, error) {
	table := Jaeckel
	if c.DirectionIntegers != nil {
		table = *c.DirectionIntegers
	}
	r, err := s.lowDiscrepancy(0, c.Dimension, c.Seed, table, !c.PlainCounter)
	if err != nil {
		return nil, err
	}
	return &SobolRsg{r}, nil
}

// NewHaltonRsg constructs the deterministic sequence without random starts or shifts.
func (s *Session) NewHaltonRsg(dimension int) (*HaltonRsg, error) {
	r, err := s.lowDiscrepancy(1, dimension, 0, Unit, false)
	if err != nil {
		return nil, err
	}
	return &HaltonRsg{r}, nil
}
func (s *Session) lowDiscrepancy(kind int, dimension int, seed uint64, table DirectionIntegers, gray bool) (randomSequence, error) {
	if dimension <= 0 {
		return randomSequence{}, fmt.Errorf("itofin: dimension must be positive")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_low_discrepancy_new(s.ctx, C.int32_t(kind), C.size_t(dimension), C.uint64_t(seed), C.int32_t(table), C.bool(gray), &id, &e), &e)
	})
	return randomSequence{object{s, uint64(id)}}, err
}
func (r SobolRsg) NextInt32Sequence() ([]uint32, error) { return r.integers(false, 0) }

// SkipTo returns raw point index. With Gray code the next draw returns that
// point before the first draw, or its successor afterwards. The plain counter
// returns the skipped-to point next. LastSequence retains the last float draw.
func (r SobolRsg) SkipTo(index uint32) ([]uint32, error) { return r.integers(true, index) }
func (r SobolRsg) integers(skip bool, index uint32) ([]uint32, error) {
	var out []uint32
	err := r.session.invoke(func() error {
		var e C.ItofinError
		var dimension C.size_t
		if err := ffiError(C.itofin_rng_sequence_dimension(r.session.ctx, C.uint64_t(r.id), &dimension, &e), &e); err != nil {
			return err
		}
		out = make([]uint32, int(dimension))
		return ffiError(C.itofin_sobol_integers(r.session.ctx, C.uint64_t(r.id), C.bool(skip), C.uint32_t(index), words(out), C.size_t(len(out)), &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return out, nil
}
