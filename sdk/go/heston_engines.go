package itofin

/*
#include "itofin.h"
*/
import "C"

// ExponentialFittingControlVariate selects the Heston quadrature control variate.
type ExponentialFittingControlVariate int32

const (
	HestonOptimal ExponentialFittingControlVariate = iota
	HestonAndersenPiterbarg
	HestonAndersenPiterbargOptCV
	HestonAsymptoticChF
	HestonAngledContour
	HestonAngledContourNoCV
)

// ExponentialFittingHestonConfig configures the control variate and contour.
// A nil configuration selects Optimal, automatic scaling and alpha=-0.5.
type ExponentialFittingHestonConfig struct {
	ControlVariate ExponentialFittingControlVariate
	Scaling        *float64
	Alpha          float64
}

type CosHestonEngine struct{ object }
type ExponentialFittingHestonEngine struct{ object }

func cosHestonConfig(l float64, n uint) C.ItofinHestonEngineConfig {
	return C.ItofinHestonEngineConfig{kind: 0, l: C.double(l), n: C.size_t(n)}
}
func exponentialHestonConfig(cfg *ExponentialFittingHestonConfig) C.ItofinHestonEngineConfig {
	out := C.ItofinHestonEngineConfig{kind: 1, alpha: -0.5}
	if cfg != nil {
		out.control_variate = C.int32_t(cfg.ControlVariate)
		out.alpha = C.double(cfg.Alpha)
		if cfg.Scaling != nil {
			out.has_scaling, out.scaling = 1, C.double(*cfg.Scaling)
		}
	}
	return out
}
func (s *Session) newHestonEngine(model *HestonModel, cfg C.ItofinHestonEngineConfig) (object, error) {
	if model == nil {
		return object{}, errNilArgument("model")
	}
	if err := sameSession(s, model.object); err != nil {
		return object{}, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_heston_engine_new(s.ctx, C.uint64_t(model.id), &cfg, &id, &e), &e)
	})
	return object{s, uint64(id)}, err
}

// NewCosHestonEngine builds a Fourier cosine engine retaining the model.
func (s *Session) NewCosHestonEngine(model *HestonModel, l float64, n uint) (*CosHestonEngine, error) {
	o, err := s.newHestonEngine(model, cosHestonConfig(l, n))
	if err != nil {
		return nil, err
	}
	return &CosHestonEngine{o}, nil
}

// NewExponentialFittingHestonEngine builds an exponentially fitted engine.
func (s *Session) NewExponentialFittingHestonEngine(model *HestonModel, cfg *ExponentialFittingHestonConfig) (*ExponentialFittingHestonEngine, error) {
	o, err := s.newHestonEngine(model, exponentialHestonConfig(cfg))
	if err != nil {
		return nil, err
	}
	return &ExponentialFittingHestonEngine{o}, nil
}
func (o *VanillaOption) SetCosHestonEngine(e *CosHestonEngine) error {
	if e == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(e.object, 2, 0)
}
func (o *VanillaOption) PriceCosHeston(e *CosHestonEngine) (float64, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(e.object, 2, 0)
}
func (o *VanillaOption) SetExponentialFittingHestonEngine(e *ExponentialFittingHestonEngine) error {
	if e == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(e.object, 2, 0)
}
func (o *VanillaOption) PriceExponentialFittingHeston(e *ExponentialFittingHestonEngine) (float64, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(e.object, 2, 0)
}
func (m *HestonModel) calibrateHestonEngine(helpers []*HestonModelHelper, method OptimizationMethod, criteria *EndCriteria, cfg C.ItofinHestonEngineConfig, options []*CalibrationOptions) error {
	if m == nil || criteria == nil {
		return errNilArgument("model, method or criteria")
	}
	methodObject, err := optimizationMethodObject(method)
	if err != nil {
		return err
	}
	objects := []object{m.object, methodObject, criteria.object}
	ids := make([]C.uint64_t, len(helpers))
	for i, h := range helpers {
		if h == nil {
			return errNilArgument("helper")
		}
		objects = append(objects, h.object)
		ids[i] = C.uint64_t(h.id)
	}
	if err := sameSession(m.session, objects...); err != nil {
		return err
	}
	args, err := newCalibrationArgs(m.session, options, false)
	if err != nil {
		return err
	}
	defer args.release()
	var ptr *C.uint64_t
	if len(ids) > 0 {
		ptr = &ids[0]
	}
	return m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_heston_calibrate_engine_with_options(m.session.ctx, C.uint64_t(m.id), ptr, C.size_t(len(ids)), C.uint64_t(methodObject.id), C.uint64_t(criteria.id), &cfg, &args.cfg, &e), &e)
	})
}

// CalibrateCOS fits the model using the COS pricing engine.
func (m *HestonModel) CalibrateCOS(helpers []*HestonModelHelper, method OptimizationMethod, criteria *EndCriteria, l float64, n uint, options ...*CalibrationOptions) error {
	return m.calibrateHestonEngine(helpers, method, criteria, cosHestonConfig(l, n), options)
}

// CalibrateExponentialFitting fits using exponentially fitted quadrature.
func (m *HestonModel) CalibrateExponentialFitting(helpers []*HestonModelHelper, method OptimizationMethod, criteria *EndCriteria, cfg *ExponentialFittingHestonConfig, options ...*CalibrationOptions) error {
	return m.calibrateHestonEngine(helpers, method, criteria, exponentialHestonConfig(cfg), options)
}

func (e *CosHestonEngine) inspector(field int32, t, u float64) (complex128, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	var real, imag C.double
	err := e.session.invoke(func() error {
		var failure C.ItofinError
		return ffiError(C.itofin_cos_heston_value(e.session.ctx, C.uint64_t(e.id), C.int32_t(field), C.double(t), C.double(u), &real, &imag, &failure), &failure)
	})
	return complex(float64(real), float64(imag)), err
}

// C1 returns normalized log-return cumulant 1 at nonnegative time.
func (e *CosHestonEngine) C1(t float64) (float64, error) {
	value, err := e.inspector(0, t, 0)
	return real(value), err
}

// C2 returns normalized log-return cumulant 2 at nonnegative time.
func (e *CosHestonEngine) C2(t float64) (float64, error) {
	value, err := e.inspector(1, t, 0)
	return real(value), err
}

// C3 returns normalized log-return cumulant 3 at nonnegative time.
func (e *CosHestonEngine) C3(t float64) (float64, error) {
	value, err := e.inspector(2, t, 0)
	return real(value), err
}

// C4 returns normalized log-return cumulant 4 at nonnegative time.
func (e *CosHestonEngine) C4(t float64) (float64, error) {
	value, err := e.inspector(3, t, 0)
	return real(value), err
}

// MuT returns the logarithm of the forward-to-spot ratio.
func (e *CosHestonEngine) MuT(t float64) (float64, error) {
	value, err := e.inspector(4, t, 0)
	return real(value), err
}

// CHF returns the normalized characteristic function at a real frequency.
func (e *CosHestonEngine) CHF(u, t float64) (complex128, error) {
	return e.inspector(5, t, u)
}
