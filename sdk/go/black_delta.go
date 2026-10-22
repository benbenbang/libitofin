package itofin

/*
#include "itofin.h"
*/
import "C"

// DeltaType is a stable Black FX delta convention.
type DeltaType int32

const (
	DeltaSpot   DeltaType = 0
	DeltaFwd    DeltaType = 1
	DeltaPaSpot DeltaType = 2
	DeltaPaFwd  DeltaType = 3
)

// AtmType is a stable ATM strike convention.
type AtmType int32

const (
	AtmNull         AtmType = 0
	AtmSpot         AtmType = 1
	AtmFwd          AtmType = 2
	AtmDeltaNeutral AtmType = 3
	AtmVegaMax      AtmType = 4
	AtmGammaMax     AtmType = 5
	AtmPutCall50    AtmType = 6
)

// BlackDeltaCalculator is immutable standalone FX delta/strike conversion.
// Construct a new calculator to change option or delta convention.
type BlackDeltaCalculator struct{ object }

// NewBlackDeltaCalculator copies positive finite spot/discounts and finite
// nonnegative stdDev (volatility times sqrt(time), not volatility alone).
func (s *Session) NewBlackDeltaCalculator(option OptionType, delta DeltaType, spot, domesticDiscount, foreignDiscount, stdDev float64) (*BlackDeltaCalculator, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_delta_new(s.ctx, C.int32_t(option), C.int32_t(delta), C.double(spot), C.double(domesticDiscount), C.double(foreignDiscount), C.double(stdDev), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackDeltaCalculator{object{s, uint64(id)}}, nil
}

func (c *BlackDeltaCalculator) calculate(kind int32, input float64, atm AtmType) (float64, error) {
	if c == nil {
		return 0, errNilArgument("calculator")
	}
	if err := sameSession(c.session, c.object); err != nil {
		return 0, err
	}
	var out C.double
	err := c.session.invoke(func() error {
		var e C.ItofinError
		switch kind {
		case 0:
			return ffiError(C.itofin_black_delta_delta_from_strike(c.session.ctx, C.uint64_t(c.id), C.double(input), &out, &e), &e)
		case 1:
			return ffiError(C.itofin_black_delta_strike_from_delta(c.session.ctx, C.uint64_t(c.id), C.double(input), &out, &e), &e)
		default:
			return ffiError(C.itofin_black_delta_atm_strike(c.session.ctx, C.uint64_t(c.id), C.int32_t(atm), &out, &e), &e)
		}
	})
	return float64(out), err
}

// DeltaFromStrike returns signed delta at finite nonnegative strike, including
// zero deviation. Premium-adjusted put deltas can be below minus one.
func (c *BlackDeltaCalculator) DeltaFromStrike(strike float64) (float64, error) {
	return c.calculate(0, strike, AtmNull)
}

// StrikeFromDelta inverts a signed interior delta with positive deviation.
// Premium-adjusted calls select the high-strike root, not the original strike.
func (c *BlackDeltaCalculator) StrikeFromDelta(delta float64) (float64, error) {
	return c.calculate(1, delta, AtmNull)
}

// AtmStrike returns an ATM strike. AtmNull errors; AtmPutCall50 requires DeltaFwd.
func (c *BlackDeltaCalculator) AtmStrike(atm AtmType) (float64, error) {
	return c.calculate(2, 0, atm)
}
