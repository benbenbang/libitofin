package itofin

/*
#include "itofin.h"
*/
import "C"

// NewCashOrNothingOption constructs a European fixed-cash digital option.
// Strike must be finite and nonnegative, and cashPayoff finite. Signed/zero
// cash is allowed. SetEngine or Price selects the analytic European engine;
// compatibility with other engines is not established by this constructor.
func (s *Session) NewCashOrNothingOption(kind OptionType, strike, cashPayoff float64, expiry Date, settings *Settings) (*VanillaOption, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_option_cash_or_nothing_new(s.ctx, C.int32_t(kind), C.double(strike), C.double(cashPayoff), C.int32_t(expiry.Serial()), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VanillaOption{object{s, uint64(id)}}, nil
}
