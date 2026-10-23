package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// FxForward retains live curves and an immutable quote-per-base spot snapshot.
// Long receives base and pays quote. Dates are explicit, fixed and unadjusted.
type FxForward struct{ object }

// FxForwardConfig has positive spot, base notional and quote-per-base strike.
// Replace a curve by constructing a new forward; spot is not a live quote.
type FxForwardConfig struct {
	BaseCurrency, QuoteCurrency      *Currency
	Spot                             float64
	BaseCurve, QuoteCurve            *YieldTermStructure
	BaseNotional, Strike             float64
	SpotSettlementDate, DeliveryDate Date
	Position                         Position
}

// NewFxForward copies the currencies and quote-per-base spot, retaining both live curves.
// It rejects mixed sessions and invalid notional, strike or explicit contractual dates.
func (s *Session) NewFxForward(a FxForwardConfig) (*FxForward, error) {
	if a.BaseCurrency == nil || a.QuoteCurrency == nil || a.BaseCurve == nil || a.QuoteCurve == nil {
		return nil, fmt.Errorf("base/quote currencies and curves required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.BaseCurrency.object, a.QuoteCurrency.object, a.BaseCurve.object, a.QuoteCurve.object); err != nil {
			return err
		}
		cfg := C.ItofinFxForwardConfig{base_currency: C.uint64_t(a.BaseCurrency.id), quote_currency: C.uint64_t(a.QuoteCurrency.id), spot: C.double(a.Spot), base_curve: C.uint64_t(a.BaseCurve.id), quote_curve: C.uint64_t(a.QuoteCurve.id), base_notional: C.double(a.BaseNotional), strike: C.double(a.Strike), spot_settlement_date: C.int32_t(a.SpotSettlementDate.Serial()), delivery_date: C.int32_t(a.DeliveryDate.Serial()), position: C.int32_t(a.Position)}
		var e C.ItofinError
		return ffiError(C.itofin_fx_forward_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FxForward{object{s, uint64(id)}}, nil
}
func (f *FxForward) value(field int32, evaluation Date) (float64, error) {
	if f == nil {
		return 0, fmt.Errorf("FX forward required")
	}
	var out C.double
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_fx_forward_value(f.session.ctx, C.uint64_t(f.id), C.int32_t(field), C.int32_t(evaluation.Serial()), &out, &e), &e)
	})
	return float64(out), err
}

// Spot returns the immutable quote-currency units per one base-currency unit.
func (f *FxForward) Spot() (float64, error) { return f.value(0, Date{}) }

// Strike returns the contractual quote-per-base delivery rate.
func (f *FxForward) Strike() (float64, error) { return f.value(1, Date{}) }

// BaseNotional returns the positive base-currency notional.
func (f *FxForward) BaseNotional() (float64, error) { return f.value(2, Date{}) }

// FairForwardRate rereads both live curves and returns a quote-per-base rate.
// It has no implied evaluation date and does not apply expiry.
func (f *FxForward) FairForwardRate() (float64, error) { return f.value(3, Date{}) }

// NPV returns quote-currency value at the quote curve reference date.
// Evaluation controls expiry only; delivery on evaluation remains active.
func (f *FxForward) NPV(evaluation Date) (float64, error) { return f.value(4, evaluation) }
func (f *FxForward) integer(field int32, evaluation Date) (int32, error) {
	if f == nil {
		return 0, fmt.Errorf("FX forward required")
	}
	var out C.int32_t
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_fx_forward_integer(f.session.ctx, C.uint64_t(f.id), C.int32_t(field), C.int32_t(evaluation.Serial()), &out, &e), &e)
	})
	return int32(out), err
}
func (f *FxForward) forwardDate(field int32) (Date, error) {
	value, err := f.integer(field, Date{})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(value)
}

// SpotSettlementDate returns the fixed, unadjusted value date of the spot snapshot.
func (f *FxForward) SpotSettlementDate() (Date, error) { return f.forwardDate(0) }

// DeliveryDate returns the fixed, unadjusted contractual delivery date.
func (f *FxForward) DeliveryDate() (Date, error) { return f.forwardDate(1) }

// Position reports whether this side receives base and pays quote (long), or the reverse.
func (f *FxForward) Position() (Position, error) {
	value, err := f.integer(2, Date{})
	return Position(value), err
}

// IsExpired is true only when delivery is strictly before evaluation.
// Delivery on evaluation remains active, independent of global event settings.
func (f *FxForward) IsExpired(evaluation Date) (bool, error) {
	value, err := f.integer(3, evaluation)
	return value != 0, err
}
func (f *FxForward) currency(side int32) (*Currency, error) {
	if f == nil {
		return nil, fmt.Errorf("FX forward required")
	}
	var id C.uint64_t
	err := f.session.invoke(func() error {
		if err := sameSession(f.session, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_fx_forward_currency(f.session.ctx, C.uint64_t(f.id), C.int32_t(side), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Currency{object{f.session, uint64(id)}}, nil
}

// BaseCurrency allocates an independent copied currency handle. Close the returned currency.
func (f *FxForward) BaseCurrency() (*Currency, error) { return f.currency(0) }

// QuoteCurrency allocates an independent copied NPV-denomination currency handle.
// Close the returned currency, which remains valid after the forward is closed.
func (f *FxForward) QuoteCurrency() (*Currency, error) { return f.currency(1) }
