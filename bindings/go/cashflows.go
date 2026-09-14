package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type IborLeg struct{ object }
type CashFlow struct{ object }
type Leg struct{ object }

func (s *Session) NewIborLeg(schedule *Schedule, index *IborIndex) (*IborLeg, error) {
	if schedule == nil || index == nil {
		return nil, fmt.Errorf("schedule and index required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, schedule.object, index.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_ibor_leg_new(s.ctx, C.uint64_t(schedule.id), C.uint64_t(index.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &IborLeg{object{s, uint64(id)}}, nil
}
func (l *IborLeg) with(field int32, value float64, integer uint64, extra ...object) (*IborLeg, error) {
	s := l.session
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, append([]object{l.object}, extra...)...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_ibor_leg_with(s.ctx, C.uint64_t(l.id), C.int32_t(field), C.double(value), C.uint64_t(integer), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &IborLeg{object{s, uint64(id)}}, nil
}
func (l *IborLeg) WithNotional(value float64) (*IborLeg, error) { return l.with(0, value, 0) }
func (l *IborLeg) WithPaymentDayCounter(d *DayCounter) (*IborLeg, error) {
	if d == nil {
		return nil, fmt.Errorf("day counter required")
	}
	return l.with(1, 0, d.id, d.object)
}
func (l *IborLeg) WithPaymentAdjustment(c BusinessDayConvention) (*IborLeg, error) {
	return l.with(2, 0, uint64(c))
}
func (l *IborLeg) WithFixingDays(days uint32) (*IborLeg, error) { return l.with(3, 0, uint64(days)) }
func (l *IborLeg) CouponCount() (int, error) {
	s := l.session
	var n C.size_t
	err := s.invoke(func() error {
		if err := sameSession(s, l.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_ibor_leg_count(s.ctx, C.uint64_t(l.id), &n, &e), &e)
	})
	return int(n), err
}
func (l *IborLeg) Build() (*Leg, error) {
	s := l.session
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, l.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_ibor_leg_build(s.ctx, C.uint64_t(l.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Leg{object{s, uint64(id)}}, nil
}
func (l *Leg) Len() (int, error) {
	s := l.session
	var n C.size_t
	err := s.invoke(func() error {
		if err := sameSession(s, l.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_leg_count(s.ctx, C.uint64_t(l.id), &n, &e), &e)
	})
	return int(n), err
}
func (l *Leg) At(index int) (*CashFlow, error) {
	s := l.session
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, l.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_leg_item(s.ctx, C.uint64_t(l.id), C.int64_t(index), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CashFlow{object{s, uint64(id)}}, nil
}
func (f *CashFlow) Amount() (float64, error) {
	s := f.session
	var n C.double
	err := s.invoke(func() error {
		if err := sameSession(s, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cashflow_amount(s.ctx, C.uint64_t(f.id), &n, &e), &e)
	})
	return float64(n), err
}
func (f *CashFlow) Date() (Date, error) {
	s := f.session
	var n C.int32_t
	err := s.invoke(func() error {
		if err := sameSession(s, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cashflow_date(s.ctx, C.uint64_t(f.id), &n, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(n))
}

type CashFlowsNPVConfig struct {
	Discount                   *YieldTermStructure
	Settings                   *Settings
	IncludeSettlementDateFlows *bool
	SettlementDate, NPVDate    *Date
}

func (l *Leg) NPV(a CashFlowsNPVConfig) (float64, error) {
	if a.Discount == nil || a.Settings == nil {
		return 0, fmt.Errorf("curve and settings required")
	}
	if (a.SettlementDate != nil && a.SettlementDate.Serial() == 0) || (a.NPVDate != nil && a.NPVDate.Serial() == 0) {
		return 0, fmt.Errorf("invalid explicit settlement or NPV date")
	}
	s := l.session
	var n C.double
	err := s.invoke(func() error {
		if err := sameSession(s, l.object, a.Discount.object, a.Settings.object); err != nil {
			return err
		}
		include := C.int32_t(-1)
		var settlement, npvDate C.int32_t
		if a.IncludeSettlementDateFlows != nil {
			include = 0
			if *a.IncludeSettlementDateFlows {
				include = 1
			}
		}
		if a.SettlementDate != nil {
			settlement = C.int32_t(a.SettlementDate.Serial())
		}
		if a.NPVDate != nil {
			npvDate = C.int32_t(a.NPVDate.Serial())
		}
		var e C.ItofinError
		return ffiError(C.itofin_leg_npv(s.ctx, C.uint64_t(l.id), C.uint64_t(a.Discount.id), C.uint64_t(a.Settings.id), include, settlement, npvDate, &n, &e), &e)
	})
	return float64(n), err
}
