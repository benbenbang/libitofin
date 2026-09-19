package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type EuropeanExercise struct{ object }
type Swaption struct{ object }
type CapFloor struct{ object }
type SettlementType int32

const (
	SettlementPhysical SettlementType = iota
	SettlementCash
)

type SettlementMethod int32

const (
	PhysicalOTC SettlementMethod = iota
	PhysicalCleared
	CollateralizedCashPrice
	ParYieldCurve
)

type CapFloorType int32

const (
	CapType CapFloorType = iota
	FloorType
	CollarType
)

func (s *Session) NewEuropeanExercise(date Date) (*EuropeanExercise, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_european_exercise_new(s.ctx, C.int32_t(date.Serial()), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &EuropeanExercise{object{s, uint64(id)}}, nil
}

type SwaptionConfig struct {
	Swap             *VanillaSwap
	Exercise         *EuropeanExercise
	SettlementType   SettlementType
	SettlementMethod SettlementMethod
	Settings         *Settings
}

func (s *Session) NewSwaption(a SwaptionConfig) (*Swaption, error) {
	if a.Swap == nil || a.Exercise == nil || a.Settings == nil {
		return nil, fmt.Errorf("swap, exercise and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Swap.object, a.Exercise.object, a.Settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_swaption_new(s.ctx, C.uint64_t(a.Swap.id), C.uint64_t(a.Exercise.id), C.int32_t(a.SettlementType), C.int32_t(a.SettlementMethod), C.uint64_t(a.Settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Swaption{object{s, uint64(id)}}, nil
}

type OisSwaptionConfig struct {
	Swap             *OvernightIndexedSwap
	Exercise         *EuropeanExercise
	SettlementType   SettlementType
	SettlementMethod SettlementMethod
	Settings         *Settings
}

func (s *Session) NewOisSwaption(a OisSwaptionConfig) (*Swaption, error) {
	if a.Swap == nil || a.Exercise == nil || a.Settings == nil {
		return nil, fmt.Errorf("swap, exercise and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Swap.object, a.Exercise.object, a.Settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_swaption_from_ois(s.ctx, C.uint64_t(a.Swap.id), C.uint64_t(a.Exercise.id), C.int32_t(a.SettlementType), C.int32_t(a.SettlementMethod), C.uint64_t(a.Settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Swaption{object{s, uint64(id)}}, nil
}

type CapFloorConfig struct {
	Type                CapFloorType
	Tenor, ForwardStart Period
	Index               *IborIndex
	Strike              float64
	Settings            *Settings
}

func (s *Session) NewCapFloor(a CapFloorConfig) (*CapFloor, error) {
	if a.Index == nil || a.Settings == nil {
		return nil, fmt.Errorf("index and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Index.object, a.Settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		c := C.ItofinCapFloorConfig{kind: C.int32_t(a.Type), tenor_length: C.int32_t(a.Tenor.Length), tenor_unit: C.int32_t(a.Tenor.Unit), forward_length: C.int32_t(a.ForwardStart.Length), forward_unit: C.int32_t(a.ForwardStart.Unit), index: C.uint64_t(a.Index.id), strike: C.double(a.Strike), settings: C.uint64_t(a.Settings.id)}
		return ffiError(C.itofin_capfloor_new(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CapFloor{object{s, uint64(id)}}, nil
}
func ratesPtr(v []float64) *C.double {
	if len(v) == 0 {
		return nil
	}
	return (*C.double)(unsafe.Pointer(&v[0]))
}
func (s *Session) capFloorFromLeg(kind CapFloorType, leg *IborLeg, caps, floors []float64, settings *Settings) (*CapFloor, error) {
	if leg == nil || settings == nil {
		return nil, fmt.Errorf("leg and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, leg.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_capfloor_from_leg(s.ctx, C.int32_t(kind), C.uint64_t(leg.id), ratesPtr(caps), C.size_t(len(caps)), ratesPtr(floors), C.size_t(len(floors)), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CapFloor{object{s, uint64(id)}}, nil
}
func (s *Session) NewCap(leg *IborLeg, rates []float64, settings *Settings) (*CapFloor, error) {
	return s.capFloorFromLeg(CapType, leg, rates, nil, settings)
}
func (s *Session) NewFloor(leg *IborLeg, rates []float64, settings *Settings) (*CapFloor, error) {
	return s.capFloorFromLeg(FloorType, leg, nil, rates, settings)
}
func (s *Session) NewCollar(leg *IborLeg, caps, floors []float64, settings *Settings) (*CapFloor, error) {
	return s.capFloorFromLeg(CollarType, leg, caps, floors, settings)
}
func rateOptionSet(o, engine object, kind int32) error {
	s := o.session
	return s.invoke(func() error {
		if err := sameSession(s, o, engine); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_rate_option_set_engine(s.ctx, C.uint64_t(o.id), C.uint64_t(engine.id), C.int32_t(kind), &e), &e)
	})
}
func (o *Swaption) SetBlackEngine(e *BlackSwaptionEngine) error {
	if e == nil {
		return fmt.Errorf("engine required")
	}
	return rateOptionSet(o.object, e.object, 0)
}
func (o *Swaption) SetBachelierEngine(e *BachelierSwaptionEngine) error {
	if e == nil {
		return fmt.Errorf("engine required")
	}
	return rateOptionSet(o.object, e.object, 1)
}
func (o *Swaption) SetJamshidianEngine(e *HullWhite) error {
	if e == nil {
		return fmt.Errorf("model required")
	}
	return rateOptionSet(o.object, e.object, 2)
}
func (o *CapFloor) SetBlackEngine(e *BlackCapFloorEngine) error {
	if e == nil {
		return fmt.Errorf("engine required")
	}
	return rateOptionSet(o.object, e.object, 3)
}

// Setting an engine and valuing execute in one worker task, so concurrent
// callers cannot replace the chosen engine between these two native calls.
func rateOptionPrice(o, engine object, engineKind, instrumentKind int32) (float64, error) {
	s := o.session
	var value C.double
	err := s.invoke(func() error {
		if err := sameSession(s, o, engine); err != nil {
			return err
		}
		var e C.ItofinError
		if err := ffiError(C.itofin_rate_option_set_engine(s.ctx, C.uint64_t(o.id), C.uint64_t(engine.id), C.int32_t(engineKind), &e), &e); err != nil {
			return err
		}
		return ffiError(C.itofin_rate_option_value(s.ctx, C.uint64_t(o.id), C.int32_t(instrumentKind), 0, &value, &e), &e)
	})
	return float64(value), err
}
func (o *Swaption) Price(e *BlackSwaptionEngine) (float64, error) {
	if o == nil || e == nil {
		return 0, fmt.Errorf("swaption and engine required")
	}
	return rateOptionPrice(o.object, e.object, 0, 0)
}
func (o *CapFloor) Price(e *BlackCapFloorEngine) (float64, error) {
	if o == nil || e == nil {
		return 0, fmt.Errorf("cap/floor and engine required")
	}
	return rateOptionPrice(o.object, e.object, 3, 1)
}
func rateOptionValue(o object, kind, field int32) (float64, error) {
	s := o.session
	var value C.double
	err := s.invoke(func() error {
		if err := sameSession(s, o); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_rate_option_value(s.ctx, C.uint64_t(o.id), C.int32_t(kind), C.int32_t(field), &value, &e), &e)
	})
	return float64(value), err
}
func rateOptionResults(o object, kind int32) (*Results, error) {
	s := o.session
	var result *Results
	err := s.invoke(func() error {
		if err := sameSession(s, o); err != nil {
			return err
		}
		var id C.uint64_t
		var e C.ItofinError
		if err := ffiError(C.itofin_rate_option_results(s.ctx, C.uint64_t(o.id), C.int32_t(kind), &id, &e), &e); err != nil {
			return err
		}
		var err error
		result, err = s.readResults(uint64(id))
		return err
	})
	return result, err
}
func (o *CapFloor) rates(which int32) ([]float64, error) {
	s := o.session
	var out []float64
	err := s.invoke(func() error {
		if err := sameSession(s, o.object); err != nil {
			return err
		}
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_capfloor_rates(s.ctx, C.uint64_t(o.id), C.int32_t(which), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		out = make([]float64, int(n))
		return ffiError(C.itofin_capfloor_rates(s.ctx, C.uint64_t(o.id), C.int32_t(which), ratesPtr(out), n, &n, &e), &e)
	})
	return out, err
}
func (o *CapFloor) CapRates() ([]float64, error)   { return o.rates(0) }
func (o *CapFloor) FloorRates() ([]float64, error) { return o.rates(1) }
func (o *CapFloor) CouponCount() (int, error) {
	n, e := rateOptionValue(o.object, 1, 3)
	return int(n), e
}
func (o *Swaption) NPV() (float64, error) { return rateOptionValue(o.object, 0, 0) }
func (o *Swaption) IsCalculated() (bool, error) {
	v, e := rateOptionValue(o.object, 0, 1)
	return v != 0, e
}
func (o *Swaption) Calculate() error           { _, e := rateOptionValue(o.object, 0, 2); return e }
func (o *Swaption) Results() (*Results, error) { return rateOptionResults(o.object, 0) }
func (o *CapFloor) NPV() (float64, error)      { return rateOptionValue(o.object, 1, 0) }
func (o *CapFloor) IsCalculated() (bool, error) {
	v, e := rateOptionValue(o.object, 1, 1)
	return v != 0, e
}
func (o *CapFloor) Calculate() error           { _, e := rateOptionValue(o.object, 1, 2); return e }
func (o *CapFloor) Results() (*Results, error) { return rateOptionResults(o.object, 1) }
