package itofin

/*
#include "itofin.h"
*/
import "C"

// BlackCalculator is an immutable plain-vanilla Black 1976 calculator, not an engine.
// Native nonfinite scalar results are errors, including zero-strike Greek NaNs.
type BlackCalculator struct{ object }

// NewBlackCalculator copies finite strike >= 0, forward > 0, stdDev >= 0 and
// discount > 0. stdDev is volatility times sqrt(time), not volatility alone.
func (s *Session) NewBlackCalculator(option OptionType, strike, forward, stdDev, discount float64) (*BlackCalculator, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_calculator_new(s.ctx, C.int32_t(option), C.double(strike), C.double(forward), C.double(stdDev), C.double(discount), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackCalculator{object{s, uint64(id)}}, nil
}

func (c *BlackCalculator) calculate(f func(*C.ItofinContext, C.uint64_t, *C.double, *C.ItofinError) C.int32_t) (float64, error) {
	if c == nil {
		return 0, errNilArgument("calculator")
	}
	if err := sameSession(c.session, c.object); err != nil {
		return 0, err
	}
	var out C.double
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(f(c.session.ctx, C.uint64_t(c.id), &out, &e), &e)
	})
	return float64(out), err
}

// Value returns present value.
func (c *BlackCalculator) Value() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_value(ctx, id, out, e)
	})
}

// DeltaForward returns forward delta.
func (c *BlackCalculator) DeltaForward() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_delta_forward(ctx, id, out, e)
	})
}

// Delta returns spot delta, requiring positive finite spot.
func (c *BlackCalculator) Delta(spot float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_delta(ctx, id, C.double(spot), out, e)
	})
}

// ElasticityForward returns forward elasticity; native finite MAX/MIN sentinels are retained.
func (c *BlackCalculator) ElasticityForward() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_elasticity_forward(ctx, id, out, e)
	})
}

// Elasticity returns spot elasticity, requiring positive finite spot.
func (c *BlackCalculator) Elasticity(spot float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_elasticity(ctx, id, C.double(spot), out, e)
	})
}

// GammaForward returns forward gamma.
func (c *BlackCalculator) GammaForward() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_gamma_forward(ctx, id, out, e)
	})
}

// Gamma returns spot gamma, requiring positive finite spot.
func (c *BlackCalculator) Gamma(spot float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_gamma(ctx, id, C.double(spot), out, e)
	})
}

// Theta returns theta per year. Native expiry shortcut is retained.
func (c *BlackCalculator) Theta(spot, maturity float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_theta(ctx, id, C.double(spot), C.double(maturity), out, e)
	})
}

// ThetaPerDay returns theta divided by 365. Native expiry shortcut is retained.
func (c *BlackCalculator) ThetaPerDay(spot, maturity float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_theta_per_day(ctx, id, C.double(spot), C.double(maturity), out, e)
	})
}

// Vega returns vega for finite nonnegative maturity.
func (c *BlackCalculator) Vega(maturity float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_vega(ctx, id, C.double(maturity), out, e)
	})
}

// Rho returns discount-rate rho for finite nonnegative maturity.
func (c *BlackCalculator) Rho(maturity float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_rho(ctx, id, C.double(maturity), out, e)
	})
}

// DividendRho returns growth-rate rho for finite nonnegative maturity.
func (c *BlackCalculator) DividendRho(maturity float64) (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_dividend_rho(ctx, id, C.double(maturity), out, e)
	})
}

// ITMCashProbability returns native N(d2), independent of option sign.
func (c *BlackCalculator) ITMCashProbability() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_itm_cash_probability(ctx, id, out, e)
	})
}

// ITMAssetProbability returns native N(d1), independent of option sign.
func (c *BlackCalculator) ITMAssetProbability() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_itm_asset_probability(ctx, id, out, e)
	})
}

// StrikeSensitivity returns strike sensitivity.
func (c *BlackCalculator) StrikeSensitivity() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_strike_sensitivity(ctx, id, out, e)
	})
}

// Alpha returns forward coefficient in the native value formula.
func (c *BlackCalculator) Alpha() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_alpha(ctx, id, out, e)
	})
}

// Beta returns strike coefficient in the native value formula.
func (c *BlackCalculator) Beta() (float64, error) {
	return c.calculate(func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_black_calculator_beta(ctx, id, out, e)
	})
}
