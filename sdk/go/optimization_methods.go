package itofin

/*
#include "itofin.h"
*/
import "C"

// OptimizationMethod is a session-owned method accepted by model calibration.
type OptimizationMethod interface {
	optimizationObject() object
}

// Simplex uses a positive characteristic length for its initial vertices.
type Simplex struct{ object }

// ConjugateGradient uses the core Armijo line search.
type ConjugateGradient struct{ object }

// SteepestDescent uses the core Armijo line search.
type SteepestDescent struct{ object }

func (m *LevenbergMarquardt) optimizationObject() object {
	if m == nil {
		return object{}
	}
	return m.object
}

func (m *Simplex) optimizationObject() object {
	if m == nil {
		return object{}
	}
	return m.object
}

func (m *ConjugateGradient) optimizationObject() object {
	if m == nil {
		return object{}
	}
	return m.object
}

func (m *SteepestDescent) optimizationObject() object {
	if m == nil {
		return object{}
	}
	return m.object
}

func optimizationMethodObject(method OptimizationMethod) (object, error) {
	if method == nil {
		return object{}, errNilArgument("method")
	}
	o := method.optimizationObject()
	if o.session == nil || o.id == 0 {
		return object{}, errNilArgument("method")
	}
	return o, nil
}

// NewSimplex requires a finite, positive characteristic length.
func (s *Session) NewSimplex(lambda float64) (*Simplex, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_simplex_new(s.ctx, C.double(lambda), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Simplex{object{s, uint64(id)}}, nil
}

// NewConjugateGradient creates a method with the default Armijo line search.
func (s *Session) NewConjugateGradient() (*ConjugateGradient, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_conjugate_gradient_new(s.ctx, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ConjugateGradient{object{s, uint64(id)}}, nil
}

// NewSteepestDescent creates a method with the default Armijo line search.
func (s *Session) NewSteepestDescent() (*SteepestDescent, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_steepest_descent_new(s.ctx, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SteepestDescent{object{s, uint64(id)}}, nil
}

// EndCriteriaType reports why the last model calibration stopped.
type EndCriteriaType int32

const (
	EndCriteriaNone EndCriteriaType = iota
	EndCriteriaMaxIterations
	EndCriteriaStationaryPoint
	EndCriteriaStationaryFunctionValue
	EndCriteriaStationaryFunctionAccuracy
	EndCriteriaZeroGradientNorm
	EndCriteriaFunctionEpsilonTooSmall
	EndCriteriaUnknown
)

func modelEndCriteriaType(o object, kind int32) (EndCriteriaType, error) {
	var value C.int32_t
	err := o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_model_end_criteria_type(o.session.ctx, C.uint64_t(o.id), C.int32_t(kind), &value, &e), &e)
	})
	return EndCriteriaType(value), err
}

func (m *HestonModel) EndCriteriaType() (EndCriteriaType, error) {
	if m == nil {
		return EndCriteriaNone, errNilArgument("model")
	}
	return modelEndCriteriaType(m.object, 0)
}

func (m *HullWhite) EndCriteriaType() (EndCriteriaType, error) {
	if m == nil {
		return EndCriteriaNone, errNilArgument("model")
	}
	return modelEndCriteriaType(m.object, 1)
}
