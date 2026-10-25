package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// CashFlowsPaymentConfig selects the evaluation context and settlement-date policy.
type CashFlowsPaymentConfig struct {
	Settings                   *Settings
	IncludeSettlementDateFlows *bool
	SettlementDate             *Date
}

func paymentDate(result C.ItofinPaymentResult, err error) (*Date, error) {
	if err != nil || result.present == 0 {
		return nil, err
	}
	date, err := DateFromSerial(int32(result.date))
	if err != nil {
		return nil, err
	}
	return &date, nil
}
func paymentAmount(result C.ItofinPaymentResult, err error) (*float64, error) {
	if err != nil || result.present == 0 {
		return nil, err
	}
	value := float64(result.amount)
	return &value, nil
}
func (l *Leg) spanDate(field int32) (Date, error) {
	if l == nil {
		return Date{}, fmt.Errorf("leg required")
	}
	var value C.int32_t
	s := l.session
	err := s.invoke(func() error {
		if err := sameSession(s, l.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_leg_span_date(s.ctx, C.uint64_t(l.id), C.int32_t(field), &value, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(value))
}

// StartDate returns the earliest coupon accrual start or non-coupon payment date.
func (l *Leg) StartDate() (Date, error) { return l.spanDate(0) }

// MaturityDate returns the latest coupon accrual end or non-coupon payment date.
func (l *Leg) MaturityDate() (Date, error) { return l.spanDate(1) }
func (l *Leg) payment(a CashFlowsPaymentConfig, direction int32, withAmount bool) (C.ItofinPaymentResult, error) {
	var result C.ItofinPaymentResult
	if l == nil || a.Settings == nil {
		return result, fmt.Errorf("leg and settings required")
	}
	settlement, err := bondDate(a.SettlementDate)
	if err != nil {
		return result, err
	}
	query := C.ItofinPaymentQuery{settings: C.uint64_t(a.Settings.id), settlement: settlement, include_settlement: -1, direction: C.int32_t(direction)}
	if a.IncludeSettlementDateFlows != nil {
		query.include_settlement = 0
		if *a.IncludeSettlementDateFlows {
			query.include_settlement = 1
		}
	}
	if withAmount {
		query.with_amount = 1
	}
	s := l.session
	err = s.invoke(func() error {
		if err := sameSession(s, l.object, a.Settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_leg_payment(s.ctx, C.uint64_t(l.id), query, &result, &e), &e)
	})
	return result, err
}

// PreviousCashFlowDate returns nil when no previous payment exists.
func (l *Leg) PreviousCashFlowDate(a CashFlowsPaymentConfig) (*Date, error) {
	return paymentDate(l.payment(a, 0, false))
}

// NextCashFlowDate returns nil when no next payment exists; it needs no fixings.
func (l *Leg) NextCashFlowDate(a CashFlowsPaymentConfig) (*Date, error) {
	return paymentDate(l.payment(a, 1, false))
}

// PreviousCashFlowAmount aggregates actual same-date payments, or returns nil.
func (l *Leg) PreviousCashFlowAmount(a CashFlowsPaymentConfig) (*float64, error) {
	return paymentAmount(l.payment(a, 0, true))
}

// NextCashFlowAmount aggregates actual same-date payments, or returns nil.
func (l *Leg) NextCashFlowAmount(a CashFlowsPaymentConfig) (*float64, error) {
	return paymentAmount(l.payment(a, 1, true))
}
func (b *Bond) inspection(field int32, settlement *Date) (int32, error) {
	if b == nil {
		return 0, fmt.Errorf("bond required")
	}
	serial, err := bondDate(settlement)
	if err != nil {
		return 0, err
	}
	var value C.int32_t
	s := b.session
	err = s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_inspection(s.ctx, C.uint64_t(b.id), C.int32_t(field), serial, &value, &e), &e)
	})
	return int32(value), err
}

// StartDate returns the accrual start without querying amounts or a pricing engine.
func (b *Bond) StartDate() (Date, error) {
	value, err := b.inspection(0, nil)
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(value)
}

// IsTradable tests outstanding notional; nil uses the calendar-aware settlement date.
func (b *Bond) IsTradable(settlement *Date) (bool, error) {
	value, err := b.inspection(1, settlement)
	return value != 0, err
}
func (b *Bond) payment(direction int32, withAmount bool, settlement *Date) (C.ItofinPaymentResult, error) {
	var result C.ItofinPaymentResult
	if b == nil {
		return result, fmt.Errorf("bond required")
	}
	serial, err := bondDate(settlement)
	if err != nil {
		return result, err
	}
	var amount C.uint8_t
	if withAmount {
		amount = 1
	}
	s := b.session
	err = s.invoke(func() error {
		if err := sameSession(s, b.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_bond_payment(s.ctx, C.uint64_t(b.id), C.int32_t(direction), amount, serial, &result, &e), &e)
	})
	return result, err
}

// PreviousCashFlowDate uses settlement exclusion, subject to the Settings today override.
func (b *Bond) PreviousCashFlowDate(settlement *Date) (*Date, error) {
	return paymentDate(b.payment(0, false, settlement))
}

// NextCashFlowDate uses settlement exclusion, subject to the Settings today override.
func (b *Bond) NextCashFlowDate(settlement *Date) (*Date, error) {
	return paymentDate(b.payment(1, false, settlement))
}

// PreviousCashFlowAmount returns actual currency units, not a per-100 price.
func (b *Bond) PreviousCashFlowAmount(settlement *Date) (*float64, error) {
	return paymentAmount(b.payment(0, true, settlement))
}

// NextCashFlowAmount returns actual currency units, not a per-100 price.
func (b *Bond) NextCashFlowAmount(settlement *Date) (*float64, error) {
	return paymentAmount(b.payment(1, true, settlement))
}
