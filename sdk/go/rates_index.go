package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type SwapIndex struct{ object }
type SwapIndexConfig struct {
	Family               string
	Tenor, FixedLegTenor Period
	SettlementDays       uint32
	Currency             *Currency
	Calendar             *Calendar
	FixedLegConvention   BusinessDayConvention
	FixedLegDayCounter   *DayCounter
	Index                *IborIndex
	Discount             *YieldTermStructure
	Settings             *Settings
}

func (s *Session) NewSwapIndex(a SwapIndexConfig) (*SwapIndex, error) {
	if a.Currency == nil || a.Calendar == nil || a.FixedLegDayCounter == nil || a.Index == nil || a.Settings == nil {
		return nil, fmt.Errorf("currency, calendar, day counter, index and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{a.Currency.object, a.Calendar.object, a.FixedLegDayCounter.object, a.Index.object, a.Settings.object}
		if a.Discount != nil {
			objects = append(objects, a.Discount.object)
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		c := C.ItofinSwapIndexConfig{tenor_length: C.int32_t(a.Tenor.Length), tenor_unit: C.int32_t(a.Tenor.Unit), settlement_days: C.uint32_t(a.SettlementDays), currency: C.uint64_t(a.Currency.id), calendar: C.uint64_t(a.Calendar.id), fixed_length: C.int32_t(a.FixedLegTenor.Length), fixed_unit: C.int32_t(a.FixedLegTenor.Unit), fixed_convention: C.int32_t(a.FixedLegConvention), day_counter: C.uint64_t(a.FixedLegDayCounter.id), index: C.uint64_t(a.Index.id), settings: C.uint64_t(a.Settings.id)}
		if a.Discount != nil {
			c.discount = C.uint64_t(a.Discount.id)
		}
		name := []byte(a.Family)
		var ptr *C.uint8_t
		if len(name) > 0 {
			ptr = (*C.uint8_t)(unsafe.Pointer(&name[0]))
		}
		var e C.ItofinError
		return ffiError(C.itofin_swap_index_new(s.ctx, ptr, C.size_t(len(name)), c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SwapIndex{object{s, uint64(id)}}, nil
}
func (i *SwapIndex) Fixing(date Date, forecastToday bool) (float64, error) {
	s := i.session
	var v C.double
	err := s.invoke(func() error {
		if err := sameSession(s, i.object); err != nil {
			return err
		}
		var flag C.uint8_t
		if forecastToday {
			flag = 1
		}
		var e C.ItofinError
		return ffiError(C.itofin_swap_index_fixing(s.ctx, C.uint64_t(i.id), C.int32_t(date.Serial()), flag, &v, &e), &e)
	})
	return float64(v), err
}
func (i *SwapIndex) Currency() (*Currency, error) {
	s := i.session
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, i.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_swap_index_currency(s.ctx, C.uint64_t(i.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Currency{object{s, uint64(id)}}, nil
}
func (i *SwapIndex) details() (C.ItofinSwapIndexDetails, error) {
	s := i.session
	var out C.ItofinSwapIndexDetails
	err := s.invoke(func() error {
		if err := sameSession(s, i.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_swap_index_details(s.ctx, C.uint64_t(i.id), &out, &e), &e)
	})
	return out, err
}
func (i *SwapIndex) FixedLegTenor() (Period, error) {
	v, e := i.details()
	return Period{Length: int32(v.fixed_length), Unit: TimeUnit(v.fixed_unit)}, e
}
func (i *SwapIndex) ExogenousDiscount() (bool, error) {
	v, e := i.details()
	return v.exogenous_discount != 0, e
}
