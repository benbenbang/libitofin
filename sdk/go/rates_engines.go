package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type BlackSwaptionEngine struct{ object }
type BachelierSwaptionEngine struct{ object }
type BlackCapFloorEngine struct{ object }
type CashAnnuityModel int32

const (
	AnnuitySwapRate CashAnnuityModel = iota
	AnnuityDiscountCurve
)

// RateEngineFlatVolConfig uses an observable quote and retains the whole curve graph.
type RateEngineFlatVolConfig struct {
	Discount     *YieldTermStructure
	Volatility   *SimpleQuote
	DayCounter   *DayCounter
	Displacement float64
	Settings     *Settings
	AnnuityModel CashAnnuityModel
}

func (s *Session) rateEngineFlat(a RateEngineFlatVolConfig, kind int32) (object, error) {
	if a.Discount == nil || a.Volatility == nil || a.DayCounter == nil || a.Settings == nil {
		return object{}, fmt.Errorf("discount, quote, day counter and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Discount.object, a.Volatility.object, a.DayCounter.object, a.Settings.object); err != nil {
			return err
		}
		c := C.ItofinRateEngineConfig{kind: C.int32_t(kind), discount: C.uint64_t(a.Discount.id), volatility: C.uint64_t(a.Volatility.id), day_counter: C.uint64_t(a.DayCounter.id), settings: C.uint64_t(a.Settings.id), displacement: C.double(a.Displacement), cash_annuity_model: C.int32_t(a.AnnuityModel), flat: 1, has_displacement: 1}
		var e C.ItofinError
		return ffiError(C.itofin_rate_engine_new(s.ctx, c, &id, &e), &e)
	})
	return object{s, uint64(id)}, err
}
func (s *Session) NewBlackSwaptionEngineFlat(a RateEngineFlatVolConfig) (*BlackSwaptionEngine, error) {
	o, e := s.rateEngineFlat(a, 0)
	if e != nil {
		return nil, e
	}
	return &BlackSwaptionEngine{o}, nil
}
func (s *Session) NewBachelierSwaptionEngineFlat(a RateEngineFlatVolConfig) (*BachelierSwaptionEngine, error) {
	o, e := s.rateEngineFlat(a, 1)
	if e != nil {
		return nil, e
	}
	return &BachelierSwaptionEngine{o}, nil
}
func (s *Session) NewBlackCapFloorEngineFlat(a RateEngineFlatVolConfig) (*BlackCapFloorEngine, error) {
	o, e := s.rateEngineFlat(a, 2)
	if e != nil {
		return nil, e
	}
	return &BlackCapFloorEngine{o}, nil
}

type SwaptionEngineConfig struct {
	Volatility   *SwaptionVolatilityStructure
	Discount     *YieldTermStructure
	Settings     *Settings
	AnnuityModel CashAnnuityModel
}

func (s *Session) swaptionEngine(a SwaptionEngineConfig, kind int32) (object, error) {
	if a.Discount == nil || a.Volatility == nil || a.Settings == nil {
		return object{}, fmt.Errorf("discount, volatility and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Discount.object, a.Volatility.object, a.Settings.object); err != nil {
			return err
		}
		c := C.ItofinRateEngineConfig{kind: C.int32_t(kind), discount: C.uint64_t(a.Discount.id), volatility: C.uint64_t(a.Volatility.id), settings: C.uint64_t(a.Settings.id), cash_annuity_model: C.int32_t(a.AnnuityModel)}
		var e C.ItofinError
		return ffiError(C.itofin_rate_engine_new(s.ctx, c, &id, &e), &e)
	})
	return object{s, uint64(id)}, err
}
func (s *Session) NewBlackSwaptionEngine(a SwaptionEngineConfig) (*BlackSwaptionEngine, error) {
	o, e := s.swaptionEngine(a, 0)
	if e != nil {
		return nil, e
	}
	return &BlackSwaptionEngine{o}, nil
}
func (s *Session) NewBachelierSwaptionEngine(a SwaptionEngineConfig) (*BachelierSwaptionEngine, error) {
	o, e := s.swaptionEngine(a, 1)
	if e != nil {
		return nil, e
	}
	return &BachelierSwaptionEngine{o}, nil
}

type BlackCapFloorEngineConfig struct {
	Volatility   *OptionletVolatilityStructure
	Discount     *YieldTermStructure
	Displacement *float64
}

func (s *Session) NewBlackCapFloorEngine(a BlackCapFloorEngineConfig) (*BlackCapFloorEngine, error) {
	if a.Discount == nil || a.Volatility == nil {
		return nil, fmt.Errorf("discount and volatility required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Discount.object, a.Volatility.object); err != nil {
			return err
		}
		c := C.ItofinRateEngineConfig{kind: 2, discount: C.uint64_t(a.Discount.id), volatility: C.uint64_t(a.Volatility.id)}
		if a.Displacement != nil {
			c.has_displacement = 1
			c.displacement = C.double(*a.Displacement)
		}
		var e C.ItofinError
		return ffiError(C.itofin_rate_engine_new(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackCapFloorEngine{object{s, uint64(id)}}, nil
}
func (e *BlackCapFloorEngine) Displacement() (float64, error) {
	s := e.session
	var v C.double
	err := s.invoke(func() error {
		if err := sameSession(s, e.object); err != nil {
			return err
		}
		var failure C.ItofinError
		return ffiError(C.itofin_black_capfloor_displacement(s.ctx, C.uint64_t(e.id), &v, &failure), &failure)
	})
	return float64(v), err
}
