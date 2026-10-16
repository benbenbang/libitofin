package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// BondForward retains a live fixed or zero-coupon plain bond and its engine.
type BondForward struct{ object }

// BondForwardConfig uses a dirty delivery quote for Strike. IncomeDiscount nil
// explicitly defaults to Discount; non-100 faces preserve native raw-income units.
type BondForwardConfig struct {
	Bond           *Bond
	ValueDate      Date
	DeliveryDate   Date
	Position       Position
	Strike         float64
	SettlementDays uint32
	DayCounter     *DayCounter
	Calendar       *Calendar
	Convention     BusinessDayConvention
	Discount       *YieldTermStructure
	IncomeDiscount *YieldTermStructure
}

func (s *Session) NewBondForward(a BondForwardConfig) (*BondForward, error) {
	if a.Bond == nil || a.DayCounter == nil || a.Calendar == nil || a.Discount == nil {
		return nil, fmt.Errorf("bond, day counter, calendar and discount required")
	}
	discount := a.Discount.object
	objects := []object{a.Bond.object, a.DayCounter.object, a.Calendar.object, discount}
	config := C.ItofinBondForwardConfig{bond: C.uint64_t(a.Bond.id), value_date: C.int32_t(a.ValueDate.Serial()), delivery_date: C.int32_t(a.DeliveryDate.Serial()), position: C.int32_t(a.Position), strike: C.double(a.Strike), settlement_days: C.uint32_t(a.SettlementDays), day_counter: C.uint64_t(a.DayCounter.id), calendar: C.uint64_t(a.Calendar.id), convention: C.int32_t(a.Convention), discount: C.uint64_t(discount.id)}
	if a.IncomeDiscount != nil {
		income := a.IncomeDiscount.object
		config.income_discount = C.uint64_t(income.id)
		objects = append(objects, income)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_forward_new(s.ctx, config, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BondForward{object{s, uint64(id)}}, nil
}
func (f *BondForward) value(field int32) (float64, error) {
	if f == nil {
		return 0, fmt.Errorf("bond forward required")
	}
	var out C.double
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_forward_value(f.session.ctx, C.uint64_t(f.id), C.int32_t(field), &out, &e), &e)
	})
	return float64(out), err
}
func (f *BondForward) NPV() (float64, error)               { return f.value(0) }
func (f *BondForward) FairForwardPrice() (float64, error)  { return f.value(1) }
func (f *BondForward) CleanForwardPrice() (float64, error) { return f.value(2) }
func (f *BondForward) integer(field int32) (int32, error) {
	if f == nil {
		return 0, fmt.Errorf("bond forward required")
	}
	var out C.int32_t
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_forward_integer(f.session.ctx, C.uint64_t(f.id), C.int32_t(field), &out, &e), &e)
	})
	return int32(out), err
}
func (f *BondForward) forwardDate(field int32) (Date, error) {
	value, err := f.integer(field)
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(value)
}
func (f *BondForward) SettlementDate() (Date, error) { return f.forwardDate(0) }
func (f *BondForward) DeliveryDate() (Date, error)   { return f.forwardDate(1) }
func (f *BondForward) ValueDate() (Date, error)      { return f.forwardDate(2) }
func (f *BondForward) IsExpired() (bool, error)      { value, err := f.integer(3); return value != 0, err }
func (f *BondForward) control(action int32) error {
	if f == nil {
		return fmt.Errorf("bond forward required")
	}
	return f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_forward_control(f.session.ctx, C.uint64_t(f.id), C.int32_t(action), &e), &e)
	})
}
func (f *BondForward) Recalculate() error { return f.control(0) }
func (f *BondForward) Freeze() error      { return f.control(1) }
func (f *BondForward) Unfreeze() error    { return f.control(2) }

func (f *BondForward) Strike() (float64, error) { return f.value(3) }
func (f *BondForward) Position() (Position, error) {
	value, err := f.integer(4)
	return Position(value), err
}
