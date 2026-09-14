package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type YoYInflationLeg struct{ object }
type YoYInflationCoupon struct{ object }
type CappedFlooredYoYInflationCoupon struct{ object }
type YoYInflationLegConfig struct {
	Schedule          *Schedule
	PaymentCalendar   *Calendar
	Index             *YoYInflationIndex
	ObservationLag    Period
	Interpolation     CpiInterpolationType
	PaymentDayCounter *DayCounter
	PaymentAdjustment BusinessDayConvention
	FixingDays        uint32
	Notional          *float64
	Notionals         []float64
	Gearing           *float64
	Gearings          []float64
	Spread            *float64
	Spreads           []float64
	Caps, Floors      []float64
}

func inflationDoubles(values []float64) *C.double {
	if len(values) == 0 {
		return nil
	}
	return (*C.double)(unsafe.Pointer(&values[0]))
}
func (s *Session) NewYoYInflationLeg(a YoYInflationLegConfig) (*YoYInflationLeg, error) {
	if a.Schedule == nil || a.PaymentCalendar == nil || a.Index == nil || a.PaymentDayCounter == nil {
		return nil, fmt.Errorf("schedule, calendar, index and day counter required")
	}
	if err := sameSession(s, a.Schedule.object, a.PaymentCalendar.object, a.Index.object, a.PaymentDayCounter.object); err != nil {
		return nil, err
	}
	// Per-coupon lists take precedence over scalar alternatives, matching Python.
	notionals := a.Notionals
	if notionals == nil && a.Notional != nil {
		notionals = []float64{*a.Notional}
	}
	gearings := a.Gearings
	if gearings == nil && a.Gearing != nil {
		gearings = []float64{*a.Gearing}
	}
	spreads := a.Spreads
	if spreads == nil && a.Spread != nil {
		spreads = []float64{*a.Spread}
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		cfg := C.ItofinYoYLegConfig{schedule: C.uint64_t(a.Schedule.id), calendar: C.uint64_t(a.PaymentCalendar.id), index: C.uint64_t(a.Index.id), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), day_counter: C.uint64_t(a.PaymentDayCounter.id), payment_convention: C.int32_t(a.PaymentAdjustment), fixing_days: C.uint32_t(a.FixingDays), notionals: inflationDoubles(notionals), notionals_len: C.size_t(len(notionals)), gearings: inflationDoubles(gearings), gearings_len: C.size_t(len(gearings)), spreads: inflationDoubles(spreads), spreads_len: C.size_t(len(spreads)), caps: inflationDoubles(a.Caps), caps_len: C.size_t(len(a.Caps)), floors: inflationDoubles(a.Floors), floors_len: C.size_t(len(a.Floors))}
		return ffiError(C.itofin_yoy_leg_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYInflationLeg{object{s, uint64(id)}}, nil
}
func (v *YoYInflationLeg) couponIDs(pricer uint64) ([]uint64, error) {
	var ids []uint64
	err := v.session.invoke(func() error {
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_leg_coupons(v.session.ctx, C.uint64_t(v.id), C.uint64_t(pricer), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		ids = make([]uint64, int(n))
		if n == 0 {
			return nil
		}
		return ffiError(C.itofin_yoy_leg_coupons(v.session.ctx, C.uint64_t(v.id), C.uint64_t(pricer), (*C.uint64_t)(unsafe.Pointer(&ids[0])), n, &n, &e), &e)
	})
	return ids, err
}
func (v *YoYInflationLeg) Coupons() ([]*YoYInflationCoupon, error) {
	ids, err := v.couponIDs(0)
	if err != nil {
		return nil, err
	}
	out := make([]*YoYInflationCoupon, len(ids))
	for i, id := range ids {
		out[i] = &YoYInflationCoupon{object{v.session, id}}
	}
	return out, nil
}
func (v *YoYInflationLeg) CappedFlooredCoupons(pricer *YoYInflationOptionletCouponPricer) ([]*CappedFlooredYoYInflationCoupon, error) {
	if pricer == nil {
		return nil, fmt.Errorf("pricer required")
	}
	if err := sameSession(v.session, pricer.object); err != nil {
		return nil, err
	}
	ids, err := v.couponIDs(pricer.id)
	if err != nil {
		return nil, err
	}
	out := make([]*CappedFlooredYoYInflationCoupon, len(ids))
	for i, id := range ids {
		out[i] = &CappedFlooredYoYInflationCoupon{object{v.session, id}}
	}
	return out, nil
}
func (v *YoYInflationLeg) Build() (*Leg, error) {
	var id C.uint64_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_leg_build(v.session.ctx, C.uint64_t(v.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Leg{object{v.session, uint64(id)}}, nil
}
func (v *YoYInflationCoupon) fields() (C.ItofinYoYCouponFields, error) {
	var out C.ItofinYoYCouponFields
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_coupon_fields(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return out, err
}
func (v *YoYInflationCoupon) value(query int32) (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_coupon_value(v.session.ctx, C.uint64_t(v.id), C.int32_t(query), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YoYInflationCoupon) DayCounter() (*DayCounter, error) {
	var id C.uint64_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_coupon_day_counter(v.session.ctx, C.uint64_t(v.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &DayCounter{object{v.session, uint64(id)}}, nil
}
func (v *YoYInflationCoupon) ObservationLag() (Period, error) {
	f, err := v.fields()
	return Period{int32(f.lag_length), TimeUnit(f.lag_unit)}, err
}
func (v *YoYInflationCoupon) Interpolation() (CpiInterpolationType, error) {
	f, err := v.fields()
	return CpiInterpolationType(f.interpolation), err
}
func (v *YoYInflationCoupon) FixingDays() (uint32, error) {
	f, err := v.fields()
	return uint32(f.fixing_days), err
}
func (v *YoYInflationCoupon) Repr() (string, error) {
	f, err := v.fields()
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("YoYInflationCoupon(%s, %s)", inflationISODate(int32(f.accrual_start)), inflationISODate(int32(f.accrual_end))), nil
}
func (v *CappedFlooredYoYInflationCoupon) fields() (C.ItofinCappedYoYCouponFields, error) {
	var out C.ItofinCappedYoYCouponFields
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_capped_yoy_coupon_fields(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return out, err
}
func (v *CappedFlooredYoYInflationCoupon) value(amount bool) (float64, error) {
	var out C.double
	var flag C.uint8_t
	if amount {
		flag = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_capped_yoy_coupon_value(v.session.ctx, C.uint64_t(v.id), flag, &out, &e), &e)
	})
	return float64(out), err
}
func (v *CappedFlooredYoYInflationCoupon) Repr() (string, error) {
	f, err := v.fields()
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("CappedFlooredYoYInflationCoupon(%s, %s)", inflationISODate(int32(f.accrual_start)), inflationISODate(int32(f.accrual_end))), nil
}
func (v *YoYInflationCoupon) Nominal() (float64, error) {
	f, err := v.fields()
	return float64(f.nominal), err
}
func (v *YoYInflationCoupon) AccrualPeriod() (float64, error) {
	f, err := v.fields()
	return float64(f.accrual_period), err
}
func (v *YoYInflationCoupon) Gearing() (float64, error) {
	f, err := v.fields()
	return float64(f.gearing), err
}
func (v *YoYInflationCoupon) Spread() (float64, error) {
	f, err := v.fields()
	return float64(f.spread), err
}
func (v *CappedFlooredYoYInflationCoupon) EffectiveCap() (float64, error) {
	f, err := v.fields()
	return float64(f.effective_cap), err
}
func (v *CappedFlooredYoYInflationCoupon) EffectiveFloor() (float64, error) {
	f, err := v.fields()
	return float64(f.effective_floor), err
}
func (v *YoYInflationCoupon) FixingDate() (Date, error) {
	f, err := v.fields()
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(f.fixing_date))
}
func (v *YoYInflationCoupon) AccrualStartDate() (Date, error) {
	f, err := v.fields()
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(f.accrual_start))
}
func (v *YoYInflationCoupon) AccrualEndDate() (Date, error) {
	f, err := v.fields()
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(f.accrual_end))
}
func (v *YoYInflationCoupon) Date() (Date, error) {
	f, err := v.fields()
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(f.payment_date))
}
func (v *YoYInflationCoupon) Rate() (float64, error)                { return v.value(0) }
func (v *YoYInflationCoupon) Amount() (float64, error)              { return v.value(1) }
func (v *YoYInflationCoupon) IndexFixing() (float64, error)         { return v.value(2) }
func (v *CappedFlooredYoYInflationCoupon) Rate() (float64, error)   { return v.value(false) }
func (v *CappedFlooredYoYInflationCoupon) Amount() (float64, error) { return v.value(true) }
func (v *CappedFlooredYoYInflationCoupon) IsCapped() (bool, error) {
	f, err := v.fields()
	return f.is_capped != 0, err
}
func (v *CappedFlooredYoYInflationCoupon) IsFloored() (bool, error) {
	f, err := v.fields()
	return f.is_floored != 0, err
}

func inflationISODate(serial int32) string {
	d := Date{serial}
	return fmt.Sprintf("%04d-%02d-%02d", d.Year(), d.Month(), d.Day())
}
