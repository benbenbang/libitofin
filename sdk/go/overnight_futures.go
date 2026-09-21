package itofin

/*
#include "itofin.h"
*/
import "C"

type OvernightIndexFuture struct{ object }
type OvernightIndexFutureRateHelper = RateHelper
type SofrFutureRateHelper = RateHelper

// OvernightFutureConfig describes a fixed reference period. Nil averaging uses Compound.
type OvernightFutureConfig struct {
	Index                   *OvernightIndex
	ValueDate, MaturityDate Date
	ConvexityAdjustment     *SimpleQuote
	AveragingMethod         *RateAveraging
}

func (s *Session) overnightFutureConfig(a OvernightFutureConfig) (C.ItofinOvernightFutureConfig, error) {
	var c C.ItofinOvernightFutureConfig
	if a.Index == nil {
		return c, errNilArgument("overnight index")
	}
	objects := []object{a.Index.object}
	if a.ConvexityAdjustment != nil {
		objects = append(objects, a.ConvexityAdjustment.object)
		c.convexity = C.uint64_t(a.ConvexityAdjustment.id)
	}
	if err := sameSession(s, objects...); err != nil {
		return c, err
	}
	c.index = C.uint64_t(a.Index.id)
	c.value_date = C.int32_t(a.ValueDate.Serial())
	c.maturity_date = C.int32_t(a.MaturityDate.Serial())
	c.averaging = C.int32_t(creditOptional(a.AveragingMethod, CompoundAveraging))
	return c, nil
}

func (s *Session) NewOvernightIndexFuture(a OvernightFutureConfig) (*OvernightIndexFuture, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		c, err := s.overnightFutureConfig(a)
		if err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_overnight_future_new(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OvernightIndexFuture{object{s, uint64(id)}}, nil
}

func (f *OvernightIndexFuture) value(field int32) (float64, error) {
	var out C.double
	s := f.session
	err := s.invoke(func() error {
		if err := sameSession(s, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_overnight_future_value(s.ctx, C.uint64_t(f.id), C.int32_t(field), &out, &e), &e)
	})
	return float64(out), err
}
func (f *OvernightIndexFuture) NPV() (float64, error)                 { return f.value(0) }
func (f *OvernightIndexFuture) ConvexityAdjustment() (float64, error) { return f.value(1) }
func (f *OvernightIndexFuture) IsExpired() (bool, error)              { v, err := f.value(2); return v != 0, err }
func (f *OvernightIndexFuture) date(field int32) (Date, error) {
	var out C.int32_t
	s := f.session
	err := s.invoke(func() error {
		if err := sameSession(s, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_overnight_future_date(s.ctx, C.uint64_t(f.id), C.int32_t(field), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(out))
}
func (f *OvernightIndexFuture) ValueDate() (Date, error)    { return f.date(0) }
func (f *OvernightIndexFuture) MaturityDate() (Date, error) { return f.date(1) }

// OvernightFutureHelperConfig adds a live market price and optional pillar.
type OvernightFutureHelperConfig struct {
	OvernightFutureConfig
	Price            *SimpleQuote
	Pillar           *Pillar
	CustomPillarDate *Date
}

func (s *Session) NewOvernightIndexFutureRateHelper(a OvernightFutureHelperConfig) (*RateHelper, error) {
	if a.Price == nil {
		return nil, errNilArgument("futures price")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		c, err := s.overnightFutureConfig(a.OvernightFutureConfig)
		if err != nil {
			return err
		}
		if err := sameSession(s, a.Price.object); err != nil {
			return err
		}
		var custom C.int32_t
		if a.CustomPillarDate != nil {
			custom = C.int32_t(a.CustomPillarDate.Serial())
		}
		var e C.ItofinError
		return ffiError(C.itofin_overnight_future_helper_new(s.ctx, c, C.uint64_t(a.Price.id), C.int32_t(creditOptional(a.Pillar, LastRelevantDate)), custom, &id, &e), &e)
	})
	return helperResult(s, id, err)
}

// SofrFutureHelperConfig selects monthly simple or quarterly compounded SOFR.
type SofrFutureHelperConfig struct {
	Price               *SimpleQuote
	ReferenceMonth      uint32
	ReferenceYear       int32
	ReferenceFrequency  Frequency
	Settings            *Settings
	ConvexityAdjustment *SimpleQuote
	Pillar              *Pillar
	CustomPillarDate    *Date
}

func (s *Session) NewSofrFutureRateHelper(a SofrFutureHelperConfig) (*RateHelper, error) {
	if a.Price == nil || a.Settings == nil {
		return nil, errNilArgument("SOFR price or settings")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{a.Price.object, a.Settings.object}
		c := C.ItofinSofrFutureHelperConfig{price: C.uint64_t(a.Price.id), month: C.uint32_t(a.ReferenceMonth), year: C.int32_t(a.ReferenceYear), frequency: C.int32_t(a.ReferenceFrequency), settings: C.uint64_t(a.Settings.id), pillar: C.int32_t(creditOptional(a.Pillar, LastRelevantDate))}
		if a.ConvexityAdjustment != nil {
			objects = append(objects, a.ConvexityAdjustment.object)
			c.convexity = C.uint64_t(a.ConvexityAdjustment.id)
		}
		if a.CustomPillarDate != nil {
			c.custom_date = C.int32_t(a.CustomPillarDate.Serial())
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_sofr_future_helper_new(s.ctx, c, &id, &e), &e)
	})
	return helperResult(s, id, err)
}

func (s *Session) NewSofr(curve *YieldTermStructure, settings *Settings) (*OvernightIndex, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{settings.object}
		var forwarding C.uint64_t
		if curve != nil {
			objects = append(objects, curve.object)
			forwarding = C.uint64_t(curve.id)
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_sofr_new(s.ctx, forwarding, C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OvernightIndex{object{s, uint64(id)}}, nil
}
func (i *OvernightIndex) AddFixing(date Date, value float64) error {
	s := i.session
	return s.invoke(func() error {
		if err := sameSession(s, i.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_overnight_add_fixing(s.ctx, C.uint64_t(i.id), C.int32_t(date.Serial()), C.double(value), &e), &e)
	})
}
