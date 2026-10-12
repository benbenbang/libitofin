package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type Bond struct{ object }
type FixedRateBond struct{ Bond }
type ZeroCouponBond struct{ Bond }
type DiscountingBondEngine struct{ object }

type BondConfig struct {
	SettlementDays    uint32
	FaceAmount        float64
	Calendar          *Calendar
	Settings          *Settings
	PaymentConvention *BusinessDayConvention
	Redemption        *float64
	IssueDate         *Date
}
type FixedRateBondConfig struct {
	BondConfig
	Schedule              *Schedule
	Coupons               []float64
	AccrualDayCounter     *DayCounter
	FirstPeriodDayCounter *DayCounter
	ExCouponPeriod        *Period
	ExCouponCalendar      *Calendar
	ExCouponConvention    *BusinessDayConvention
	ExCouponEndOfMonth    bool
}
type ZeroCouponBondConfig struct {
	BondConfig
	MaturityDate Date
}

func bondConfig(a BondConfig) (C.ItofinBondConfig, []object, error) {
	if a.Settings == nil {
		return C.ItofinBondConfig{}, nil, fmt.Errorf("settings required")
	}
	c := C.ItofinBondConfig{settlement_days: C.uint32_t(a.SettlementDays), face_amount: C.double(a.FaceAmount), redemption: 100, settings: C.uint64_t(a.Settings.id), payment_convention: C.int32_t(Following)}
	objects := []object{a.Settings.object}
	if a.Calendar != nil {
		c.calendar = C.uint64_t(a.Calendar.id)
		objects = append(objects, a.Calendar.object)
	}
	if a.PaymentConvention != nil {
		c.payment_convention = C.int32_t(*a.PaymentConvention)
	}
	if a.Redemption != nil {
		c.redemption = C.double(*a.Redemption)
	}
	if a.IssueDate != nil {
		c.issue_date = C.int32_t(a.IssueDate.Serial())
		if c.issue_date == 0 {
			return c, nil, fmt.Errorf("invalid issue date")
		}
	}
	return c, objects, nil
}
func (s *Session) NewZeroCouponBond(a ZeroCouponBondConfig) (*ZeroCouponBond, error) {
	if a.Calendar == nil {
		return nil, fmt.Errorf("calendar required")
	}
	c, objects, err := bondConfig(a.BondConfig)
	if err != nil {
		return nil, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_zero_coupon_bond_new(s.ctx, c, C.int32_t(a.MaturityDate.Serial()), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ZeroCouponBond{Bond{object{s, uint64(id)}}}, nil
}
func (s *Session) NewFixedRateBond(a FixedRateBondConfig) (*FixedRateBond, error) {
	if a.Schedule == nil || a.AccrualDayCounter == nil {
		return nil, fmt.Errorf("schedule and accrual day counter required")
	}
	c, objects, err := bondConfig(a.BondConfig)
	if err != nil {
		return nil, err
	}
	objects = append(objects, a.Schedule.object, a.AccrualDayCounter.object)
	config := C.ItofinFixedBondConfig{bond: c, schedule: C.uint64_t(a.Schedule.id), day_counter: C.uint64_t(a.AccrualDayCounter.id), ex_coupon_convention: C.int32_t(Unadjusted)}
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
	var id C.uint64_t
	err = s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_fixed_rate_bond_new(s.ctx, config, (*C.double)(unsafe.Pointer(unsafe.SliceData(rates))), C.size_t(len(rates)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FixedRateBond{Bond{object{s, uint64(id)}}}, nil
}
func (s *Session) NewDiscountingBondEngine(discount *YieldTermStructure, settings *Settings, includeSettlementDateFlows *bool) (*DiscountingBondEngine, error) {
	if discount == nil || settings == nil {
		return nil, fmt.Errorf("discount curve and settings required")
	}
	include := C.int32_t(-1)
	if includeSettlementDateFlows != nil {
		include = 0
		if *includeSettlementDateFlows {
			include = 1
		}
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, discount.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_discounting_bond_engine_new(s.ctx, C.uint64_t(discount.id), C.uint64_t(settings.id), include, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &DiscountingBondEngine{object{s, uint64(id)}}, nil
}
func (b *Bond) SetEngine(engine *DiscountingBondEngine) error {
	if b == nil || engine == nil {
		return fmt.Errorf("bond and engine required")
	}
	s := b.session
	return s.invoke(func() error {
		if err := sameSession(s, b.object, engine.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_set_engine(s.ctx, C.uint64_t(b.id), C.uint64_t(engine.id), &e), &e)
	})
}
func bondDate(d *Date) (C.int32_t, error) {
	if d == nil {
		return 0, nil
	}
	if d.Serial() == 0 {
		return 0, fmt.Errorf("invalid date")
	}
	return C.int32_t(d.Serial()), nil
}
func (b *Bond) real(field int32, date *Date) (float64, error) {
	if b == nil {
		return 0, fmt.Errorf("bond required")
	}
	serial, err := bondDate(date)
	if err != nil {
		return 0, err
	}
	var out C.double
	s := b.session
	err = s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_real(s.ctx, C.uint64_t(b.id), C.int32_t(field), serial, &out, &e), &e)
	})
	return float64(out), err
}
func (b *Bond) integer(field int32, date *Date) (int32, error) {
	if b == nil {
		return 0, fmt.Errorf("bond required")
	}
	serial, err := bondDate(date)
	if err != nil {
		return 0, err
	}
	var out C.int32_t
	s := b.session
	err = s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_integer(s.ctx, C.uint64_t(b.id), C.int32_t(field), serial, &out, &e), &e)
	})
	return int32(out), err
}
func (b *Bond) handle(field int32) (object, error) {
	if b == nil {
		return object{}, fmt.Errorf("bond required")
	}
	var out C.uint64_t
	s := b.session
	err := s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_handle(s.ctx, C.uint64_t(b.id), C.int32_t(field), &out, &e), &e)
	})
	return object{s, uint64(out)}, err
}
func (b *Bond) action(action int32) error {
	if b == nil {
		return fmt.Errorf("bond required")
	}
	s := b.session
	return s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_action(s.ctx, C.uint64_t(b.id), C.int32_t(action), &e), &e)
	})
}
func (b *Bond) NPV() (float64, error)                     { return b.real(0, nil) }
func (b *Bond) SettlementValue() (float64, error)         { return b.real(1, nil) }
func (b *Bond) CleanPrice() (float64, error)              { return b.real(2, nil) }
func (b *Bond) DirtyPrice() (float64, error)              { return b.real(3, nil) }
func (b *Bond) AccruedAmount(date *Date) (float64, error) { return b.real(4, date) }
func (b *Bond) Notional(date *Date) (float64, error)      { return b.real(5, date) }
func (b *Bond) SettlementDays() (uint32, error)           { v, e := b.integer(0, nil); return uint32(v), e }
func (b *Bond) SettlementDate(reference *Date) (Date, error) {
	v, e := b.integer(1, reference)
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(v)
}
func (b *Bond) MaturityDate() (Date, error) {
	v, e := b.integer(2, nil)
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(v)
}
func (b *Bond) IssueDate() (*Date, error) {
	v, e := b.integer(3, nil)
	if e != nil || v == 0 {
		return nil, e
	}
	d, e := DateFromSerial(v)
	return &d, e
}
func (b *Bond) IsExpired() (bool, error)    { v, e := b.integer(4, nil); return v != 0, e }
func (b *Bond) IsCalculated() (bool, error) { v, e := b.integer(5, nil); return v != 0, e }
func (b *Bond) Calendar() (*Calendar, error) {
	v, e := b.handle(0)
	if e != nil {
		return nil, e
	}
	return &Calendar{v}, nil
}
func (b *Bond) Cashflows() (*Leg, error) {
	v, e := b.handle(1)
	if e != nil {
		return nil, e
	}
	return &Leg{v}, nil
}
func (b *Bond) Redemptions() (*Leg, error) {
	v, e := b.handle(2)
	if e != nil {
		return nil, e
	}
	return &Leg{v}, nil
}
func (b *Bond) Recalculate() error { return b.action(0) }
func (b *Bond) Freeze() error      { return b.action(1) }
func (b *Bond) Unfreeze() error    { return b.action(2) }
func (b *FixedRateBond) Frequency() (Frequency, error) {
	v, e := b.integer(6, nil)
	return Frequency(v), e
}
func (b *FixedRateBond) DayCounter() (*DayCounter, error) {
	v, e := b.handle(3)
	if e != nil {
		return nil, e
	}
	return &DayCounter{v}, nil
}
func (b *FixedRateBond) FirstPeriodDayCounter() (*DayCounter, error) {
	v, e := b.handle(4)
	if e != nil || v.id == 0 {
		return nil, e
	}
	return &DayCounter{v}, nil
}
