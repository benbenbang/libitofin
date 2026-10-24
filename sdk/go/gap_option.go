package itofin

/*
#include "itofin.h"
*/
import "C"

// NewGapOption constructs a European gap option with separate trigger and
// payment strikes. The trigger must be finite and nonnegative; secondStrike
// must be finite and may be signed. Trigger equality is included and negative
// payments are not floored. Use SetFdEngine or PriceFd; the analytic Black
// engine rejects Gap. Existing FD grid and cell-averaging limits are retained.
func (s *Session) NewGapOption(kind OptionType, strike, secondStrike float64, expiry Date, settings *Settings) (*VanillaOption, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_gap_new(s.ctx, C.int32_t(kind), C.double(strike), C.double(secondStrike), C.int32_t(expiry.Serial()), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VanillaOption{object{s, uint64(id)}}, nil
}
