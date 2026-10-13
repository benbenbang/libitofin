package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type CallabilityType int32

const (
	CallabilityCall CallabilityType = 0
	CallabilityPut  CallabilityType = 1
)

type Callability struct {
	Price     float64
	PriceType BondPriceType
	Type      CallabilityType
	Date      Date
}
type CallabilitySchedule []Callability

type CallableFixedRateBond struct{ Bond }
type CallableZeroCouponBond struct{ Bond }
type CallableFixedRateBondConfig struct {
	FixedRateBondConfig
	Callability CallabilitySchedule
}
type CallableZeroCouponBondConfig struct {
	ZeroCouponBondConfig
	DayCounter  *DayCounter
	Callability CallabilitySchedule
}

func callabilityRecord(a Callability) C.ItofinCallability {
	return C.ItofinCallability{price: C.double(a.Price), price_type: C.int32_t(a.PriceType), call_type: C.int32_t(a.Type), date: C.int32_t(a.Date.Serial())}
}
func callabilityValue(a C.ItofinCallability) (Callability, error) {
	d, err := DateFromSerial(int32(a.date))
	return Callability{float64(a.price), BondPriceType(a.price_type), CallabilityType(a.call_type), d}, err
}
func NewCallability(price float64, priceType BondPriceType, callType CallabilityType, date Date) (Callability, error) {
	var out C.ItofinCallability
	var e C.ItofinError
	if err := ffiError(C.itofin_callability_new(callabilityRecord(Callability{price, priceType, callType, date}), &out, &e), &e); err != nil {
		return Callability{}, err
	}
	return callabilityValue(out)
}
func callabilityRecords(a CallabilitySchedule) []C.ItofinCallability {
	out := make([]C.ItofinCallability, len(a))
	for i, call := range a {
		out[i] = callabilityRecord(call)
	}
	return out
}
func (s *Session) NewCallableFixedRateBond(a CallableFixedRateBondConfig) (*CallableFixedRateBond, error) {
	if a.Schedule == nil || a.AccrualDayCounter == nil {
		return nil, fmt.Errorf("schedule and accrual day counter required")
	}
	if a.Calendar != nil || a.FirstPeriodDayCounter != nil {
		return nil, fmt.Errorf("callable fixed bond does not support calendar or first-period overrides")
	}
	c, objects, err := bondConfig(a.BondConfig)
	if err != nil {
		return nil, err
	}
	objects = append(objects, a.Schedule.object, a.AccrualDayCounter.object)
	config := C.ItofinFixedBondConfig{bond: c, schedule: C.uint64_t(a.Schedule.id), day_counter: C.uint64_t(a.AccrualDayCounter.id), ex_coupon_convention: C.int32_t(Unadjusted)}
	if a.ExCouponPeriod != nil {
		config.has_ex_coupon = 1
		if a.ExCouponCalendar != nil {
			config.ex_coupon_calendar = C.uint64_t(a.ExCouponCalendar.id)
			objects = append(objects, a.ExCouponCalendar.object)
		}
		config.ex_coupon_length = C.int32_t(a.ExCouponPeriod.Length)
		config.ex_coupon_unit = C.int32_t(a.ExCouponPeriod.Unit)
	}
	if a.ExCouponConvention != nil {
		config.ex_coupon_convention = C.int32_t(*a.ExCouponConvention)
	}
	if a.ExCouponEndOfMonth {
		config.ex_coupon_end_of_month = 1
	}
	rates := make([]C.double, len(a.Coupons))
	for i, r := range a.Coupons {
		rates[i] = C.double(r)
	}
	calls := callabilityRecords(a.Callability)
	var id C.uint64_t
	err = s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_callable_fixed_rate_bond_new(s.ctx, config, (*C.double)(unsafe.Pointer(unsafe.SliceData(rates))), C.size_t(len(rates)), (*C.ItofinCallability)(unsafe.Pointer(unsafe.SliceData(calls))), C.size_t(len(calls)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CallableFixedRateBond{Bond{object{s, uint64(id)}}}, nil
}
func (s *Session) NewCallableZeroCouponBond(a CallableZeroCouponBondConfig) (*CallableZeroCouponBond, error) {
	if a.Calendar == nil || a.DayCounter == nil {
		return nil, fmt.Errorf("calendar and day counter required")
	}
	c, objects, err := bondConfig(a.BondConfig)
	if err != nil {
		return nil, err
	}
	objects = append(objects, a.DayCounter.object)
	calls := callabilityRecords(a.Callability)
	var id C.uint64_t
	err = s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_callable_zero_coupon_bond_new(s.ctx, c, C.int32_t(a.MaturityDate.Serial()), C.uint64_t(a.DayCounter.id), (*C.ItofinCallability)(unsafe.Pointer(unsafe.SliceData(calls))), C.size_t(len(calls)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CallableZeroCouponBond{Bond{object{s, uint64(id)}}}, nil
}
func (b *Bond) callability() (CallabilitySchedule, error) {
	if b == nil {
		return nil, fmt.Errorf("bond required")
	}
	var out CallabilitySchedule
	s := b.session
	err := s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var count C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_callable_bond_callability(s.ctx, C.uint64_t(b.id), nil, 0, &count, &e), &e); err != nil {
			return err
		}
		records := make([]C.ItofinCallability, int(count))
		if count != 0 {
			if err := ffiError(C.itofin_callable_bond_callability(s.ctx, C.uint64_t(b.id), (*C.ItofinCallability)(unsafe.Pointer(unsafe.SliceData(records))), count, &count, &e), &e); err != nil {
				return err
			}
		}
		out = make(CallabilitySchedule, len(records))
		for i, record := range records {
			value, err := callabilityValue(record)
			if err != nil {
				return err
			}
			out[i] = value
		}
		return nil
	})
	return out, err
}
func (b *CallableFixedRateBond) Callability() (CallabilitySchedule, error) {
	if b == nil {
		return nil, fmt.Errorf("bond required")
	}
	return b.Bond.callability()
}
func (b *CallableZeroCouponBond) Callability() (CallabilitySchedule, error) {
	if b == nil {
		return nil, fmt.Errorf("bond required")
	}
	return b.Bond.callability()
}
func (b *CallableFixedRateBond) Frequency() (Frequency, error) {
	if b == nil {
		return 0, fmt.Errorf("bond required")
	}
	v, e := b.integer(6, nil)
	return Frequency(v), e
}
func (b *CallableFixedRateBond) DayCounter() (*DayCounter, error) {
	if b == nil {
		return nil, fmt.Errorf("bond required")
	}
	v, e := b.handle(3)
	if e != nil {
		return nil, e
	}
	return &DayCounter{v}, nil
}
func (b *CallableZeroCouponBond) DayCounter() (*DayCounter, error) {
	if b == nil {
		return nil, fmt.Errorf("bond required")
	}
	var id C.uint64_t
	s := b.session
	err := s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_callable_zero_bond_day_counter(s.ctx, C.uint64_t(b.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &DayCounter{object{s, uint64(id)}}, nil
}
