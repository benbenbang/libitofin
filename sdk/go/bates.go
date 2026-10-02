package itofin

/*
#include "itofin.h"
*/
import "C"

// BatesProcess retains live market inputs and constant Heston/lognormal-jump parameters.
// It is an analytic carrier, not a forecast-path simulator.
type BatesProcess struct{ object }

// BatesModel owns eight calibrated parameters and retains observable market inputs.
type BatesModel struct{ object }

// BatesEngine prices European plain-vanilla NPV. Greeks are unavailable.
type BatesEngine struct{ object }

// BatesProcessConfig uses physical constructor order; Lambda is annual jump
// intensity, Nu is mean log jump and Delta is log-jump standard deviation.
type BatesProcessConfig struct {
	Spot                                            *SimpleQuote
	RiskFree, Dividend                              *YieldTermStructure
	V0, Kappa, Theta, Sigma, Rho, Lambda, Nu, Delta float64
}

// NewBatesProcess retains live spot and curves even after external handles close.
func (s *Session) NewBatesProcess(cfg BatesProcessConfig) (*BatesProcess, error) {
	if cfg.Spot == nil || cfg.RiskFree == nil || cfg.Dividend == nil {
		return nil, errNilArgument("spot or curve")
	}
	if err := sameSession(s, cfg.Spot.object, cfg.RiskFree.object, cfg.Dividend.object); err != nil {
		return nil, err
	}
	parameters := C.ItofinBatesParameters{v0: C.double(cfg.V0), kappa: C.double(cfg.Kappa), theta: C.double(cfg.Theta), sigma: C.double(cfg.Sigma), rho: C.double(cfg.Rho), lambda: C.double(cfg.Lambda), nu: C.double(cfg.Nu), delta: C.double(cfg.Delta)}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_process_new(s.ctx, C.uint64_t(cfg.Spot.id), C.uint64_t(cfg.RiskFree.id), C.uint64_t(cfg.Dividend.id), &parameters, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BatesProcess{object{s, uint64(id)}}, nil
}

// NewBatesModel seeds the calibrated model from its retained process.
func (s *Session) NewBatesModel(process *BatesProcess) (*BatesModel, error) {
	if process == nil {
		return nil, errNilArgument("process")
	}
	if err := sameSession(s, process.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_model_new(s.ctx, C.uint64_t(process.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BatesModel{object{s, uint64(id)}}, nil
}

func batesParameter(o object, kind int32, field uint) (float64, error) {
	if err := sameSession(o.session, o); err != nil {
		return 0, err
	}
	var value C.double
	err := o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_parameter(o.session.ctx, C.uint64_t(o.id), C.int32_t(kind), C.size_t(field), &value, &e), &e)
	})
	return float64(value), err
}

func (p *BatesProcess) parameter(field uint) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	return batesParameter(p.object, 0, field)
}
func (m *BatesModel) parameter(field uint) (float64, error) {
	if m == nil {
		return 0, errNilArgument("model")
	}
	return batesParameter(m.object, 1, field)
}

// V0 returns the current V0 parameter.
func (p *BatesProcess) V0() (float64, error) { return p.parameter(0) }

// Kappa returns the current Kappa parameter.
func (p *BatesProcess) Kappa() (float64, error) { return p.parameter(1) }

// Theta returns the current Theta parameter.
func (p *BatesProcess) Theta() (float64, error) { return p.parameter(2) }

// Sigma returns the current Sigma parameter.
func (p *BatesProcess) Sigma() (float64, error) { return p.parameter(3) }

// Rho returns the current Rho parameter.
func (p *BatesProcess) Rho() (float64, error) { return p.parameter(4) }

// Lambda returns the current Lambda parameter.
func (p *BatesProcess) Lambda() (float64, error) { return p.parameter(5) }

// Nu returns the current Nu parameter.
func (p *BatesProcess) Nu() (float64, error) { return p.parameter(6) }

// Delta returns the current Delta parameter.
func (p *BatesProcess) Delta() (float64, error) { return p.parameter(7) }

// V0 returns the current V0 parameter.
func (p *BatesModel) V0() (float64, error) { return p.parameter(0) }

// Kappa returns the current Kappa parameter.
func (p *BatesModel) Kappa() (float64, error) { return p.parameter(1) }

// Theta returns the current Theta parameter.
func (p *BatesModel) Theta() (float64, error) { return p.parameter(2) }

// Sigma returns the current Sigma parameter.
func (p *BatesModel) Sigma() (float64, error) { return p.parameter(3) }

// Rho returns the current Rho parameter.
func (p *BatesModel) Rho() (float64, error) { return p.parameter(4) }

// Lambda returns the current Lambda parameter.
func (p *BatesModel) Lambda() (float64, error) { return p.parameter(5) }

// Nu returns the current Nu parameter.
func (p *BatesModel) Nu() (float64, error) { return p.parameter(6) }

// Delta returns the current Delta parameter.
func (p *BatesModel) Delta() (float64, error) { return p.parameter(7) }

// InitialValues returns an owned array containing current spot and initial variance.
func (p *BatesProcess) InitialValues() ([2]float64, error) {
	var out [2]float64
	if p == nil {
		return out, errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return out, err
	}
	var values [2]C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_process_initial_values(p.session.ctx, C.uint64_t(p.id), &values[0], 2, &e), &e)
	})
	for i, v := range values {
		out[i] = float64(v)
	}
	return out, err
}

// Spot reads the current retained quote.
func (p *BatesProcess) Spot() (float64, error) {
	values, err := p.InitialValues()
	return values[0], err
}

// Time converts a date using the retained risk-free curve's day counter.
func (p *BatesProcess) Time(date Date) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return 0, err
	}
	var value C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_process_time(p.session.ctx, C.uint64_t(p.id), C.int32_t(date.Serial()), &value, &e), &e)
	})
	return float64(value), err
}

// Params returns a copied theta,kappa,sigma,rho,v0,nu,delta,lambda array.
func (m *BatesModel) Params() ([]float64, error) {
	if m == nil {
		return nil, errNilArgument("model")
	}
	if err := sameSession(m.session, m.object); err != nil {
		return nil, err
	}
	var values [8]C.double
	err := m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_model_params(m.session.ctx, C.uint64_t(m.id), &values[0], 8, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	out := make([]float64, 8)
	for i, v := range values {
		out[i] = float64(v)
	}
	return out, nil
}

// SetParams atomically replaces theta,kappa,sigma,rho,v0,nu,delta,lambda.
// Inputs are copied before native use and never retained.
func (m *BatesModel) SetParams(parameters []float64) error {
	if m == nil {
		return errNilArgument("model")
	}
	if err := sameSession(m.session, m.object); err != nil {
		return err
	}
	values := make([]C.double, len(parameters))
	for i, v := range parameters {
		values[i] = C.double(v)
	}
	var ptr *C.double
	if len(values) > 0 {
		ptr = &values[0]
	}
	return m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_model_set_params(m.session.ctx, C.uint64_t(m.id), ptr, C.size_t(len(values)), &e), &e)
	})
}

// NewBatesEngine retains its live model. Order must be 1..192; 144 is conventional.
func (s *Session) NewBatesEngine(model *BatesModel, order uint) (*BatesEngine, error) {
	if model == nil {
		return nil, errNilArgument("model")
	}
	if err := sameSession(s, model.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_engine_new(s.ctx, C.uint64_t(model.id), C.size_t(order), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BatesEngine{object{s, uint64(id)}}, nil
}

// SetBatesEngine attaches a retained analytic Bates engine.
func (o *VanillaOption) SetBatesEngine(engine *BatesEngine) error {
	if engine == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(engine.object, 2, 0)
}

// PriceBates attaches and prices in one session operation.
func (o *VanillaOption) PriceBates(engine *BatesEngine) (float64, error) {
	if engine == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(engine.object, 2, 0)
}
