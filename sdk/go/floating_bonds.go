package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"math"
	"unsafe"
)

type FloatingRateBond struct{ Bond }
type BlackIborCouponPricer struct{ object }

type FloatingRateBondConfig struct {
	SettlementDays     uint32
	FaceAmount         float64
	Schedule           *Schedule
	Index              *IborIndex
	AccrualDayCounter  *DayCounter
	Settings           *Settings
	PaymentConvention  *BusinessDayConvention
	FixingDays         *uint32
	Gearings, Spreads  []float64
	Caps, Floors       []*float64
	Redemption         *float64
	IssueDate          *Date
	ExCouponPeriod     *Period
	ExCouponCalendar   *Calendar
	ExCouponConvention *BusinessDayConvention
	ExCouponEndOfMonth bool
	FixingConvention   *BusinessDayConvention
}

func floatingValues(values []float64) []C.double {
	out := make([]C.double, len(values))
	for i, v := range values {
		out[i] = C.double(v)
	}
	return out
}
func floatingOptions(values []*float64) []C.double {
	out := make([]C.double, len(values))
	for i, v := range values {
		if v == nil {
			out[i] = C.double(math.MaxFloat32)
		} else {
			out[i] = C.double(*v)
		}
	}
	return out
}
func (s *Session) NewFloatingRateBond(a FloatingRateBondConfig) (*FloatingRateBond, error) {
	if a.Schedule == nil || a.Index == nil || a.AccrualDayCounter == nil {
		return nil, fmt.Errorf("schedule, index and accrual day counter required")
	}
	common, objects, err := bondConfig(BondConfig{SettlementDays: a.SettlementDays, FaceAmount: a.FaceAmount, Settings: a.Settings, PaymentConvention: a.PaymentConvention, Redemption: a.Redemption, IssueDate: a.IssueDate})
	if err != nil {
		return nil, err
	}
	objects = append(objects, a.Schedule.object, a.Index.object, a.AccrualDayCounter.object)
	config := C.ItofinFloatingBondConfig{bond: common, schedule: C.uint64_t(a.Schedule.id), index: C.uint64_t(a.Index.id), day_counter: C.uint64_t(a.AccrualDayCounter.id), fixing_convention: C.int32_t(Preceding), ex_coupon_convention: C.int32_t(Unadjusted)}
	if a.FixingDays != nil {
		config.has_fixing_days = 1
		config.fixing_days = C.uint32_t(*a.FixingDays)
	}
	if a.FixingConvention != nil {
		config.fixing_convention = C.int32_t(*a.FixingConvention)
	}
	if a.ExCouponPeriod != nil {
		config.has_ex_coupon = 1
		config.ex_coupon_length = C.int32_t(a.ExCouponPeriod.Length)
		config.ex_coupon_unit = C.int32_t(a.ExCouponPeriod.Unit)

	}
	if a.ExCouponCalendar != nil {
		objects = append(objects, a.ExCouponCalendar.object)
		config.ex_coupon_calendar = C.uint64_t(a.ExCouponCalendar.id)
	}
	if a.ExCouponConvention != nil {
		config.ex_coupon_convention = C.int32_t(*a.ExCouponConvention)
	}
	if a.ExCouponEndOfMonth {
		config.ex_coupon_end_of_month = 1
	}
	gearings, spreads, caps, floors := floatingValues(a.Gearings), floatingValues(a.Spreads), floatingOptions(a.Caps), floatingOptions(a.Floors)
	vectors := C.ItofinFloatingBondVectors{gearings: (*C.double)(unsafe.Pointer(unsafe.SliceData(gearings))), gearings_count: C.size_t(len(gearings)), spreads: (*C.double)(unsafe.Pointer(unsafe.SliceData(spreads))), spreads_count: C.size_t(len(spreads)), caps: (*C.double)(unsafe.Pointer(unsafe.SliceData(caps))), caps_count: C.size_t(len(caps)), floors: (*C.double)(unsafe.Pointer(unsafe.SliceData(floors))), floors_count: C.size_t(len(floors))}
	var id C.uint64_t
	err = s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_floating_rate_bond_new(s.ctx, config, vectors, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FloatingRateBond{Bond{object{s, uint64(id)}}}, nil
}
func (s *Session) NewBlackIborCouponPricer(volatility *OptionletVolatilityStructure) (*BlackIborCouponPricer, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var vol C.uint64_t
		if volatility != nil {
			if err := sameSession(s, volatility.object); err != nil {
				return err
			}
			vol = C.uint64_t(volatility.id)
		}
		var e C.ItofinError
		return ffiError(C.itofin_black_ibor_coupon_pricer_new(s.ctx, vol, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackIborCouponPricer{object{s, uint64(id)}}, nil
}
func (b *FloatingRateBond) SetCouponPricer(pricer *BlackIborCouponPricer) error {
	if b == nil || pricer == nil {
		return fmt.Errorf("floating bond and coupon pricer required")
	}
	s := b.session
	return s.invoke(func() error {
		if err := sameSession(s, b.object, pricer.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_floating_bond_set_pricer(s.ctx, C.uint64_t(b.id), C.uint64_t(pricer.id), &e), &e)
	})
}
func (b *FloatingRateBond) Index() (*IborIndex, error) {
	if b == nil {
		return nil, fmt.Errorf("floating bond required")
	}
	var id C.uint64_t
	s := b.session
	err := s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_floating_bond_index(s.ctx, C.uint64_t(b.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &IborIndex{object{s, uint64(id)}}, nil
}
