package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

// AmortizingFloatingRateBondConfig supplies exact per-period balances for a plain Ibor bond.
// Settlement, coupon and index fixing lags are bounded by the supported Date span.
type AmortizingFloatingRateBondConfig struct {
	SettlementDays     uint32
	Notionals          []float64
	Schedule           *Schedule
	Index              *IborIndex
	AccrualDayCounter  *DayCounter
	Settings           *Settings
	PaymentConvention  *BusinessDayConvention
	FixingDays         *uint32
	Spreads            []float64
	IssueDate          *Date
	ExCouponPeriod     *Period
	ExCouponCalendar   *Calendar
	ExCouponConvention *BusinessDayConvention
	ExCouponEndOfMonth bool
	FixingConvention   *BusinessDayConvention
}

// NewAmortizingFloatingRateBond copies positive nonincreasing balances and repays principal at par.
func (s *Session) NewAmortizingFloatingRateBond(a AmortizingFloatingRateBondConfig) (*FloatingRateBond, error) {
	if a.Schedule == nil || a.Index == nil || a.AccrualDayCounter == nil || a.Settings == nil {
		return nil, fmt.Errorf("schedule, index, accrual day counter and settings required")
	}
	issue, err := bondDate(a.IssueDate)
	if err != nil {
		return nil, err
	}
	objects := []object{a.Schedule.object, a.Index.object, a.AccrualDayCounter.object, a.Settings.object}
	config := C.ItofinAmortizingFloatingBondConfig{
		settlement_days: C.uint32_t(a.SettlementDays),
		schedule:        C.uint64_t(a.Schedule.id), index: C.uint64_t(a.Index.id),
		day_counter: C.uint64_t(a.AccrualDayCounter.id), settings: C.uint64_t(a.Settings.id),
		payment_convention: C.int32_t(Following), issue_date: issue,
		fixing_convention: C.int32_t(Preceding), ex_coupon_convention: C.int32_t(Unadjusted),
	}
	if a.PaymentConvention != nil {
		config.payment_convention = C.int32_t(*a.PaymentConvention)
	}
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
	notionals, spreads := floatingValues(a.Notionals), floatingValues(a.Spreads)
	var id C.uint64_t
	err = s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_amortizing_floating_rate_bond_new(
			s.ctx, config, (*C.double)(unsafe.Pointer(unsafe.SliceData(notionals))), C.size_t(len(notionals)),
			(*C.double)(unsafe.Pointer(unsafe.SliceData(spreads))), C.size_t(len(spreads)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FloatingRateBond{Bond{object{s, uint64(id)}}}, nil
}
