package itofin

/*
#include "itofin.h"
*/
import "C"

// OptionType follows the Python enum ordering.
type OptionType int32

const (
	Call OptionType = iota
	Put
)

type VanillaOption struct{ object }

func (s *Session) NewVanillaOption(kind OptionType, strike float64, expiry Date, settings *Settings) (*VanillaOption, error) {
	return s.newOption(kind, strike, Date{}, expiry, false, settings)
}
func (s *Session) NewAmericanOption(kind OptionType, strike float64, earliest, latest Date, settings *Settings) (*VanillaOption, error) {
	return s.newOption(kind, strike, earliest, latest, true, settings)
}
func (s *Session) newOption(kind OptionType, strike float64, earliest, expiry Date, american bool, settings *Settings) (*VanillaOption, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	var flag C.int32_t
	if american {
		flag = 1
	}
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_new(s.ctx, C.int32_t(kind), C.double(strike), C.int32_t(earliest.Serial()), C.int32_t(expiry.Serial()), flag, C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VanillaOption{object{s, uint64(id)}}, nil
}
func (o *VanillaOption) setEngine(source object, kind int32, order uint) error {
	if o == nil {
		return errNilArgument("option")
	}
	s := o.session
	if err := sameSession(s, o.object, source); err != nil {
		return err
	}
	return s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_set_engine(s.ctx, C.uint64_t(o.id), C.uint64_t(source.id), C.int32_t(kind), C.size_t(order), &e), &e)
	})
}
func (o *VanillaOption) SetEngine(p *BlackScholesProcess) error {
	if p == nil {
		return errNilArgument("process")
	}
	return o.setEngine(p.object, 0, 0)
}
func (o *VanillaOption) SetHestonEngine(m *HestonModel, order uint) error {
	if m == nil {
		return errNilArgument("model")
	}
	return o.setEngine(m.object, 1, order)
}
func (o *VanillaOption) SetMCEngine(e *MCEuropeanEngine) error {
	if e == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(e.object, 2, 0)
}
func (o *VanillaOption) SetMCHestonEngine(e *MCEuropeanHestonEngine) error {
	if e == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(e.object, 2, 0)
}
func (o *VanillaOption) SetMCAmericanEngine(e *MCAmericanEngine) error {
	if e == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(e.object, 2, 0)
}
func (o *VanillaOption) value(field int32) (float64, error) {
	if o == nil {
		return 0, errNilArgument("option")
	}
	var value C.double
	err := o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_value(o.session.ctx, C.uint64_t(o.id), C.int32_t(field), &value, &e), &e)
	})
	return float64(value), err
}
func (o *VanillaOption) NPV() (float64, error)                 { return o.value(0) }
func (o *VanillaOption) Delta() (float64, error)               { return o.value(1) }
func (o *VanillaOption) Gamma() (float64, error)               { return o.value(2) }
func (o *VanillaOption) Theta() (float64, error)               { return o.value(3) }
func (o *VanillaOption) Vega() (float64, error)                { return o.value(4) }
func (o *VanillaOption) Rho() (float64, error)                 { return o.value(5) }
func (o *VanillaOption) DividendRho() (float64, error)         { return o.value(6) }
func (o *VanillaOption) ErrorEstimate() (float64, error)       { return o.value(7) }
func (o *VanillaOption) ExerciseProbability() (float64, error) { return o.value(8) }
func (o *VanillaOption) cache(calculate bool) (bool, error) {
	if o == nil {
		return false, errNilArgument("option")
	}
	var flag, out C.int32_t
	if calculate {
		flag = 1
	}
	err := o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_calculate(o.session.ctx, C.uint64_t(o.id), flag, &out, &e), &e)
	})
	return out != 0, err
}
func (o *VanillaOption) Calculate() error            { _, err := o.cache(true); return err }
func (o *VanillaOption) IsCalculated() (bool, error) { return o.cache(false) }

// Results returns a frozen native valuation snapshot.
func (o *VanillaOption) Results() (*Results, error) {
	if o == nil {
		return nil, errNilArgument("option")
	}
	var result *Results
	err := o.session.invoke(func() error {
		var e C.ItofinError
		var id C.uint64_t
		if err := ffiError(C.itofin_option_results(o.session.ctx, C.uint64_t(o.id), &id, &e), &e); err != nil {
			return err
		}
		var err error
		result, err = o.session.readResults(uint64(id))
		return err
	})
	return result, err
}

// One-shot methods attach and price within one session operation, so concurrent
// callers cannot replace the engine between attachment and valuation.
func (o *VanillaOption) price(source object, kind int32, order uint) (float64, error) {
	if o == nil {
		return 0, errNilArgument("option")
	}
	s := o.session
	if err := sameSession(s, o.object, source); err != nil {
		return 0, err
	}
	var value C.double
	err := s.invoke(func() error {
		var e C.ItofinError
		if err := ffiError(C.itofin_option_set_engine(s.ctx, C.uint64_t(o.id), C.uint64_t(source.id), C.int32_t(kind), C.size_t(order), &e), &e); err != nil {
			return err
		}
		return ffiError(C.itofin_option_value(s.ctx, C.uint64_t(o.id), 0, &value, &e), &e)
	})
	return float64(value), err
}
func (o *VanillaOption) Price(p *BlackScholesProcess) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	return o.price(p.object, 0, 0)
}
func (o *VanillaOption) PriceHeston(m *HestonModel, order uint) (float64, error) {
	if m == nil {
		return 0, errNilArgument("model")
	}
	return o.price(m.object, 1, order)
}
func (o *VanillaOption) PriceMC(e *MCEuropeanEngine) (float64, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(e.object, 2, 0)
}
func (o *VanillaOption) PriceMCHeston(e *MCEuropeanHestonEngine) (float64, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(e.object, 2, 0)
}
func (o *VanillaOption) PriceMCAmerican(e *MCAmericanEngine) (float64, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(e.object, 2, 0)
}
