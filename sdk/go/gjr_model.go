package itofin

/*
#include "itofin.h"
*/
import "C"

// GJRModel retains live market inputs and six daily-variance parameters.
type GJRModel struct{ object }

// NewGJRModel seeds a model from a retained process. Model processes use full truncation.
func (s *Session) NewGJRModel(process *GJRProcess) (*GJRModel, error) {
	if process == nil {
		return nil, errNilArgument("process")
	}
	if err := sameSession(s, process.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_model_new(s.ctx, C.uint64_t(process.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GJRModel{object{s, uint64(id)}}, nil
}

// Params returns a copied omega,alpha,beta,gamma,lambda,v0 array. V0 is daily variance.
func (m *GJRModel) Params() ([]float64, error) {
	if m == nil {
		return nil, errNilArgument("model")
	}
	if err := sameSession(m.session, m.object); err != nil {
		return nil, err
	}
	out := make([]float64, 6)
	err := m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_model_params(m.session.ctx, C.uint64_t(m.id), doubles(out), C.size_t(len(out)), &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return out, nil
}

// SetParams atomically replaces omega,alpha,beta,gamma,lambda,v0. Inputs are copied.
func (m *GJRModel) SetParams(parameters []float64) error {
	if m == nil {
		return errNilArgument("model")
	}
	if err := sameSession(m.session, m.object); err != nil {
		return err
	}
	values := append([]float64(nil), parameters...)
	return m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_model_set_params(m.session.ctx, C.uint64_t(m.id), doubles(values), C.size_t(len(values)), &e), &e)
	})
}

// Process returns an independently closeable handle to the current model process.
// Each model update replaces this process; an earlier returned handle stays unchanged.
func (m *GJRModel) Process() (*GJRProcess, error) {
	if m == nil {
		return nil, errNilArgument("model")
	}
	if err := sameSession(m.session, m.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_model_process(m.session.ctx, C.uint64_t(m.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GJRProcess{object{m.session, uint64(id)}}, nil
}
