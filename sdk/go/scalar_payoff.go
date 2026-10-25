package itofin

/*
#include "itofin.h"
*/
import "C"

// PayoffKind identifies one of the five supported immutable scalar payoffs.
type PayoffKind int32

const (
	PayoffPlainVanilla PayoffKind = iota
	PayoffCashOrNothing
	PayoffAssetOrNothing
	PayoffGap
	PayoffPercentageStrike
)

// ScalarPayoffParameters is a copy of the stored terms. Extra is the cash
// amount for cash-or-nothing, the payment strike for gap, and zero otherwise.
// Strike is dimensionless moneyness for percentage-strike, not percent points.
type ScalarPayoffParameters struct {
	Kind       PayoffKind
	OptionType OptionType
	Strike     float64
	Extra      float64
}

// ScalarPayoff evaluates an immutable standalone payoff, not a pricing engine.
// Terms and evaluated prices must be finite; signed values are allowed.
type ScalarPayoff struct{ object }

func (s *Session) newScalarPayoff(kind PayoffKind, option OptionType, strike, extra float64) (*ScalarPayoff, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_scalar_payoff_new(s.ctx, C.int32_t(kind), C.int32_t(option), C.double(strike), C.double(extra), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ScalarPayoff{object{s, uint64(id)}}, nil
}

// NewPlainVanillaPayoff copies finite strike and floors intrinsic value at zero.
func (s *Session) NewPlainVanillaPayoff(option OptionType, strike float64) (*ScalarPayoff, error) {
	return s.newScalarPayoff(PayoffPlainVanilla, option, strike, 0)
}

// NewCashOrNothingPayoff pays cash strictly beyond strike, zero at equality.
func (s *Session) NewCashOrNothingPayoff(option OptionType, strike, cash float64) (*ScalarPayoff, error) {
	return s.newScalarPayoff(PayoffCashOrNothing, option, strike, cash)
}

// NewAssetOrNothingPayoff pays the underlying strictly beyond strike.
func (s *Session) NewAssetOrNothingPayoff(option OptionType, strike float64) (*ScalarPayoff, error) {
	return s.newScalarPayoff(PayoffAssetOrNothing, option, strike, 0)
}

// NewGapPayoff uses an inclusive trigger and a separate payment strike.
// Negative payouts are retained, not floored.
func (s *Session) NewGapPayoff(option OptionType, strike, secondStrike float64) (*ScalarPayoff, error) {
	return s.newScalarPayoff(PayoffGap, option, strike, secondStrike)
}

// NewPercentageStrikePayoff copies dimensionless moneyness, not percent points.
// The floored ratio factor multiplies the signed underlying price.
func (s *Session) NewPercentageStrikePayoff(option OptionType, moneyness float64) (*ScalarPayoff, error) {
	return s.newScalarPayoff(PayoffPercentageStrike, option, moneyness, 0)
}

// Value evaluates at a finite signed price and rejects nonfinite results.
func (p *ScalarPayoff) Value(price float64) (float64, error) {
	if p == nil {
		return 0, errNilArgument("payoff")
	}
	var out C.double
	err := p.session.invoke(func() error {
		if err := sameSession(p.session, p.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_scalar_payoff_value(p.session.ctx, C.uint64_t(p.id), C.double(price), &out, &e), &e)
	})
	return float64(out), err
}

// Parameters returns a copy of stored terms without evaluating the payoff.
func (p *ScalarPayoff) Parameters() (ScalarPayoffParameters, error) {
	if p == nil {
		return ScalarPayoffParameters{}, errNilArgument("payoff")
	}
	var out C.ItofinScalarPayoffInfo
	err := p.session.invoke(func() error {
		if err := sameSession(p.session, p.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_scalar_payoff_info(p.session.ctx, C.uint64_t(p.id), &out, &e), &e)
	})
	return ScalarPayoffParameters{Kind: PayoffKind(out.kind), OptionType: OptionType(out.option_kind), Strike: float64(out.strike), Extra: float64(out.extra)}, err
}
