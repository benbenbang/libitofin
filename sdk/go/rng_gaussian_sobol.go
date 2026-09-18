package itofin

/*
#include "itofin.h"
*/
import "C"

type GaussianLowDiscrepancySequenceGenerator struct{ randomSequence }

// NewGaussianLowDiscrepancySequenceGenerator copies Sobol state and maps draws
// through the inverse standard normal CDF.
func (s *Session) NewGaussianLowDiscrepancySequenceGenerator(source *SobolRsg) (*GaussianLowDiscrepancySequenceGenerator, error) {
	if source == nil {
		return nil, errNilArgument("source")
	}
	if err := sameSession(s, source.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gaussian_sobol_new(s.ctx, C.uint64_t(source.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GaussianLowDiscrepancySequenceGenerator{randomSequence{object{s, uint64(id)}}}, nil
}
