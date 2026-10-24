package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

// AmortizingFixedRateBondConfig supplies exact positive nonincreasing balances,
// one per schedule accrual period. Principal reductions and final redemption
// are paid at par. Prices retain Bond's current-notional, per-100 convention.
type AmortizingFixedRateBondConfig struct {
	SettlementDays        uint32
	Notionals             []float64
	Calendar              *Calendar
	Settings              *Settings
	PaymentConvention     *BusinessDayConvention
	IssueDate             *Date
	Schedule              *Schedule
	Coupons               []float64
	AccrualDayCounter     *DayCounter
	FirstPeriodDayCounter *DayCounter
	ExCouponPeriod        *Period
	ExCouponCalendar      *Calendar
	ExCouponConvention    *BusinessDayConvention
	ExCouponEndOfMonth    bool
}

// NewAmortizingFixedRateBond constructs a par-amortizing fixed bond, retaining
// the existing fixed-bond metadata and common Bond valuation interface.
func (s *Session) NewAmortizingFixedRateBond(a AmortizingFixedRateBondConfig) (*FixedRateBond, error) {
	if a.Schedule == nil || a.AccrualDayCounter == nil {
		return nil, fmt.Errorf("schedule and accrual day counter required")
	}
	c, objects, err := bondConfig(BondConfig{SettlementDays: a.SettlementDays, FaceAmount: 1,
		Calendar: a.Calendar, Settings: a.Settings, PaymentConvention: a.PaymentConvention,
		IssueDate: a.IssueDate})
	if err != nil {
		return nil, err
	}
	objects = append(objects, a.Schedule.object, a.AccrualDayCounter.object)
	config := C.ItofinAmortizingFixedBondConfig{settlement_days: c.settlement_days, settings: c.settings, issue_date: c.issue_date, payment_convention: c.payment_convention, calendar: c.calendar, schedule: C.uint64_t(a.Schedule.id), day_counter: C.uint64_t(a.AccrualDayCounter.id), ex_coupon_convention: C.int32_t(Unadjusted)}
	if a.FirstPeriodDayCounter != nil {
		config.first_period_day_counter = C.uint64_t(a.FirstPeriodDayCounter.id)
		objects = append(objects, a.FirstPeriodDayCounter.object)
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
	for i, r := range a.Coupons {
		rates[i] = C.double(r)
	}
	notionals := make([]C.double, len(a.Notionals))
	for i, n := range a.Notionals {
		notionals[i] = C.double(n)
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_amortizing_fixed_rate_bond_new(s.ctx, config, (*C.double)(unsafe.Pointer(unsafe.SliceData(notionals))), C.size_t(len(notionals)), (*C.double)(unsafe.Pointer(unsafe.SliceData(rates))), C.size_t(len(rates)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FixedRateBond{Bond{object{s, uint64(id)}}}, nil
}
