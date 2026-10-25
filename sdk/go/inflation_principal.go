package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// InflationCashFlow is an immutable explicit-base CPI or live-base inflation
// principal. Index fixings and linked inflation curves remain live and retained.
type InflationCashFlow struct{ object }

// CPICashFlowConfig specifies a fixed denominator and live lagged CPI numerator.
// GrowthOnly subtracts the uninflated nominal; false pays the full CPI ratio.
type CPICashFlowConfig struct {
	Index                        *ZeroInflationIndex
	Nominal, BaseCPI             float64
	ObservationDate, PaymentDate Date
	ObservationLag               Period
	Interpolation                CpiInterpolationType
	GrowthOnly                   bool
}

// ZeroInflationCashFlowConfig observes both StartDate and EndDate less the lag.
// Base fixing corrections affect the denominator. Construction reads no fixings.
type ZeroInflationCashFlowConfig struct {
	Index                           *ZeroInflationIndex
	Nominal                         float64
	StartDate, EndDate, PaymentDate Date
	ObservationLag                  Period
	Interpolation                   CpiInterpolationType
	GrowthOnly                      bool
}

// NewCPICashFlow retains the index and its Settings. Invalid dates, non-finite
// parameters and zero/near-zero explicit base CPI fail before creating a handle.
func (s *Session) NewCPICashFlow(a CPICashFlowConfig) (*InflationCashFlow, error) {
	if a.Index == nil {
		return nil, fmt.Errorf("inflation index required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Index.object); err != nil {
			return err
		}
		cfg := C.ItofinCpiCashFlowConfig{index: C.uint64_t(a.Index.id), nominal: C.double(a.Nominal), base_cpi: C.double(a.BaseCPI), observation_date: C.int32_t(a.ObservationDate.Serial()), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), payment_date: C.int32_t(a.PaymentDate.Serial()), growth_only: creditBool(a.GrowthOnly)}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_cashflow_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &InflationCashFlow{object{s, uint64(id)}}, nil
}

// NewZeroInflationCashFlow checks both observation-date subtractions before
// constructing the live-base flow. Missing historical fixings remain lazy errors.
func (s *Session) NewZeroInflationCashFlow(a ZeroInflationCashFlowConfig) (*InflationCashFlow, error) {
	if a.Index == nil {
		return nil, fmt.Errorf("inflation index required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Index.object); err != nil {
			return err
		}
		cfg := C.ItofinZeroInflationCashFlowConfig{index: C.uint64_t(a.Index.id), nominal: C.double(a.Nominal), start_date: C.int32_t(a.StartDate.Serial()), end_date: C.int32_t(a.EndDate.Serial()), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), payment_date: C.int32_t(a.PaymentDate.Serial()), growth_only: creditBool(a.GrowthOnly)}
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_cashflow_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &InflationCashFlow{object{s, uint64(id)}}, nil
}

func (f *InflationCashFlow) value(field int32) (float64, error) {
	if f == nil {
		return 0, errNilArgument("inflation cash flow")
	}
	var out C.double
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_inflation_cashflow_value(f.session.ctx, C.uint64_t(f.id), C.int32_t(field), &out, &e), &e)
	})
	return float64(out), err
}

// Amount returns an undiscounted finite payment or an error.
func (f *InflationCashFlow) Amount() (float64, error) { return f.value(0) }

// BaseFixing returns the fixed explicit base or reads the live lagged start CPI.
func (f *InflationCashFlow) BaseFixing() (float64, error) { return f.value(1) }

// IndexFixing reads the live lagged numerator CPI.
func (f *InflationCashFlow) IndexFixing() (float64, error) { return f.value(2) }

// Date returns the payment date without resolving inflation fixings.
func (f *InflationCashFlow) Date() (Date, error) {
	if f == nil {
		return Date{}, errNilArgument("inflation cash flow")
	}
	var out C.int32_t
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_inflation_cashflow_date(f.session.ctx, C.uint64_t(f.id), &out, &e), &e)
	})
	return Date{int32(out)}, err
}

func (f *InflationCashFlow) convert(asLeg bool) (object, error) {
	if f == nil {
		return object{}, errNilArgument("inflation cash flow")
	}
	var out C.uint64_t
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_inflation_cashflow_convert(f.session.ctx, C.uint64_t(f.id), creditBool(asLeg), &out, &e), &e)
	})
	return object{f.session, uint64(out)}, err
}

// AsCashFlow returns an independently retained generic finite-checked flow.
func (f *InflationCashFlow) AsCashFlow() (*CashFlow, error) {
	o, err := f.convert(false)
	if err != nil {
		return nil, err
	}
	return &CashFlow{o}, nil
}

// AsLeg returns a retained one-flow leg. NPV requires the same Settings instance
// used to construct the inflation index, not merely an equal evaluation date.
func (f *InflationCashFlow) AsLeg() (*Leg, error) {
	o, err := f.convert(true)
	if err != nil {
		return nil, err
	}
	return &Leg{o}, nil
}
