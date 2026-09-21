package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type BermudanExercise struct{ object }
type TreeSwaptionEngine struct{ object }

// NewBermudanExercise copies and sorts dates; an empty schedule is invalid.
func (s *Session) NewBermudanExercise(dates []Date) (*BermudanExercise, error) {
	serials := make([]C.int32_t, len(dates))
	for i, d := range dates {
		serials[i] = C.int32_t(d.Serial())
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bermudan_exercise_new(s.ctx, (*C.int32_t)(unsafe.Pointer(unsafe.SliceData(serials))), C.size_t(len(serials)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BermudanExercise{object{s, uint64(id)}}, nil
}

func (e *BermudanExercise) Dates() ([]Date, error) {
	var dates []Date
	err := e.session.invoke(func() error {
		var failure C.ItofinError
		var count C.size_t
		if err := ffiError(C.itofin_bermudan_exercise_dates(e.session.ctx, C.uint64_t(e.id), nil, 0, &count, &failure), &failure); err != nil {
			return err
		}
		serials := make([]C.int32_t, int(count))
		if err := ffiError(C.itofin_bermudan_exercise_dates(e.session.ctx, C.uint64_t(e.id), (*C.int32_t)(unsafe.Pointer(unsafe.SliceData(serials))), count, &count, &failure), &failure); err != nil {
			return err
		}
		dates = make([]Date, len(serials))
		for i, d := range serials {
			dates[i] = Date{int32(d)}
		}
		return nil
	})
	return dates, err
}

type BermudanSwaptionConfig struct {
	Swap             *VanillaSwap
	Exercise         *BermudanExercise
	SettlementType   SettlementType
	SettlementMethod SettlementMethod
	Settings         *Settings
}

func (s *Session) NewBermudanSwaption(a BermudanSwaptionConfig) (*Swaption, error) {
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

func (s *Session) NewTreeSwaptionEngine(model *HullWhite, timeSteps int, settings *Settings) (*TreeSwaptionEngine, error) {
	if model == nil || settings == nil || timeSteps <= 0 {
		return nil, fmt.Errorf("model, settings and positive time steps required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, model.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_tree_swaption_engine_new(s.ctx, C.uint64_t(model.id), C.size_t(timeSteps), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &TreeSwaptionEngine{object{s, uint64(id)}}, nil
}

func (o *Swaption) SetTreeEngine(e *TreeSwaptionEngine) error {
	if e == nil {
		return fmt.Errorf("engine required")
	}
	return rateOptionSet(o.object, e.object, 6)
}

// SetUsingAtParCoupons selects forecasting before construction; it does not invalidate cached prices.
func (s *Settings) SetUsingAtParCoupons(value bool) error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_settings_set_using_at_par_coupons(s.session.ctx, C.uint64_t(s.id), C.bool(value), &e), &e)
	})
}
func (s *Settings) UsingAtParCoupons() (bool, error) {
	var value C.bool
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_settings_using_at_par_coupons(s.session.ctx, C.uint64_t(s.id), &value, &e), &e)
	})
	return bool(value), err
}
