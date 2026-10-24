package itofin

/*
#include "itofin.h"
*/
import "C"

// NewAssetOrNothingOption constructs a European asset-or-nothing option.
// Strike must be finite and nonnegative. Strictly in the money the option pays
// the terminal asset price; equality pays zero. Use SetFdEngine or PriceFd.
// Analytic Black pricing rejects this payoff; grid convergence still applies.
func (s *Session) NewAssetOrNothingOption(kind OptionType, strike float64, expiry Date, settings *Settings) (*VanillaOption, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_asset_or_nothing_new(s.ctx, C.int32_t(kind), C.double(strike), C.int32_t(expiry.Serial()), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VanillaOption{object{s, uint64(id)}}, nil
}
