package itofin

/*
#include "itofin.h"
*/
import "C"

// NewPercentageStrikeOption constructs a European percentage-strike option.
// Moneyness must be finite and is a dimensionless ratio, not a percent scaled
// by 100 or an absolute strike. Signed ratios are accepted. Call pays
// S*max(1-m,0), put S*max(m-1,0). Use SetFdEngine or PriceFd with constant
// volatility. The FD volatility lookup uses m unchanged; strike-smile surfaces
// may reject it. Analytic Black pricing rejects this payoff.
func (s *Session) NewPercentageStrikeOption(kind OptionType, moneyness float64, expiry Date, settings *Settings) (*VanillaOption, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_percentage_strike_new(s.ctx, C.int32_t(kind), C.double(moneyness), C.int32_t(expiry.Serial()), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VanillaOption{object{s, uint64(id)}}, nil
}
