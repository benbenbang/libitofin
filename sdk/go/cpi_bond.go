package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

// CPIBond retains the shared Bond interface and pays full indexed principal.
type CPIBond struct{ Bond }

// CPIBondConfig describes plain CPI-scaled coupons with an explicit base CPI.
// Prices are per 100 of original face, not inflation-adjusted principal.
type CPIBondConfig struct {
	SettlementDays           uint32
	FaceAmount               float64
	BaseCPI                  float64
	ObservationLag           Period
	Index                    *ZeroInflationIndex
	ObservationInterpolation CpiInterpolationType
	Schedule                 *Schedule
	Coupons                  []float64
	AccrualDayCounter        *DayCounter
	Settings                 *Settings
	PaymentConvention        *BusinessDayConvention
	PaymentCalendar          *Calendar
	IssueDate                *Date
	ExCouponPeriod           *Period
	ExCouponCalendar         *Calendar
	ExCouponConvention       *BusinessDayConvention
	ExCouponEndOfMonth       bool
}

// NewCPIBond builds the modern explicit-base CPI bond. Missing fixings are
// reported on valuation, not construction. Deprecated growth-only is unsupported.
func (s *Session) NewCPIBond(a CPIBondConfig) (*CPIBond, error) {
	if a.Index == nil || a.Schedule == nil || a.AccrualDayCounter == nil || a.Settings == nil {
		return nil, fmt.Errorf("index, schedule, accrual day counter and settings required")
	}
	config := C.ItofinCpiBondConfig{
		settlement_days: C.uint32_t(a.SettlementDays), face_amount: C.double(a.FaceAmount), base_cpi: C.double(a.BaseCPI),
		observation_lag_length: C.int32_t(a.ObservationLag.Length), observation_lag_unit: C.int32_t(a.ObservationLag.Unit),
		index: C.uint64_t(a.Index.id), observation_interpolation: C.int32_t(a.ObservationInterpolation),
		schedule: C.uint64_t(a.Schedule.id), day_counter: C.uint64_t(a.AccrualDayCounter.id), settings: C.uint64_t(a.Settings.id),
		payment_convention: C.int32_t(ModifiedFollowing), ex_coupon_convention: C.int32_t(Unadjusted),
	}
	objects := []object{a.Index.object, a.Schedule.object, a.AccrualDayCounter.object, a.Settings.object}
	if a.PaymentCalendar != nil {
		config.payment_calendar = C.uint64_t(a.PaymentCalendar.id)
		objects = append(objects, a.PaymentCalendar.object)
	}
	if a.PaymentConvention != nil {
		config.payment_convention = C.int32_t(*a.PaymentConvention)
	}
	if a.IssueDate != nil {
		config.issue_date = C.int32_t(a.IssueDate.Serial())
		if config.issue_date == 0 {
			return nil, fmt.Errorf("invalid issue date")
		}
	}
	if a.ExCouponPeriod != nil {
		if a.ExCouponCalendar == nil {
			return nil, fmt.Errorf("ex-coupon calendar required")
		}
		config.has_ex_coupon = 1
		config.ex_coupon_calendar = C.uint64_t(a.ExCouponCalendar.id)
		config.ex_coupon_length = C.int32_t(a.ExCouponPeriod.Length)
		config.ex_coupon_unit = C.int32_t(a.ExCouponPeriod.Unit)
		objects = append(objects, a.ExCouponCalendar.object)
	}
	if a.ExCouponConvention != nil {
		config.ex_coupon_convention = C.int32_t(*a.ExCouponConvention)
	}
	if a.ExCouponEndOfMonth {
		config.ex_coupon_end_of_month = 1
	}
	rates := make([]C.double, len(a.Coupons))
	for i, rate := range a.Coupons {
		rates[i] = C.double(rate)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_bond_new(s.ctx, config, (*C.double)(unsafe.Pointer(unsafe.SliceData(rates))), C.size_t(len(rates)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CPIBond{Bond{object{s, uint64(id)}}}, nil
}
