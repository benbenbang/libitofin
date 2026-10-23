package itofin

/*
#include "itofin.h"
*/
import "C"

// XccyBasisSwap pays base and receives quote, with constant notionals and principal exchanges.
// It retains live curves/indexes and immutable quote-per-base spot, without a pricing cache.
type XccyBasisSwap struct{ object }

// XccyBasisSwapConfig defines plain Ibor legs; notionals need not match at spot.
// Nil PaymentConvention means Following. Nil Valuation uses live Settings dates/policies.
type XccyBasisSwapConfig struct {
	BaseSchedule, QuoteSchedule                    *Schedule
	BaseIndex, QuoteIndex                          *IborIndex
	BaseCurve, QuoteCurve                          *YieldTermStructure
	BaseDayCounter, QuoteDayCounter                *DayCounter
	BaseCurrency, QuoteCurrency                    *Currency
	BaseNotional, QuoteNotional, QuoteSpread, Spot float64
	Settings                                       *Settings
	PaymentConvention                              *BusinessDayConvention
	Valuation                                      *XccyBasisValuation
	SpotSettlementDate                             *Date
}

// XccyBasisValuation fixes settlement/NPV dates; today's Settings override still takes precedence.
type XccyBasisValuation struct {
	SettlementDate, NPVDate Date
	IncludeSettlementFlows  bool
}

// NewXccyBasisSwap copies currencies/spot and retains exact live dependency handles.
func (s *Session) NewXccyBasisSwap(a XccyBasisSwapConfig) (*XccyBasisSwap, error) {
	if a.BaseSchedule == nil || a.QuoteSchedule == nil || a.BaseIndex == nil || a.QuoteIndex == nil || a.BaseCurve == nil || a.QuoteCurve == nil || a.BaseDayCounter == nil || a.QuoteDayCounter == nil || a.BaseCurrency == nil || a.QuoteCurrency == nil || a.Settings == nil {
		return nil, errNilArgument("cross-currency swap argument")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.BaseSchedule.object, a.QuoteSchedule.object, a.BaseIndex.object, a.QuoteIndex.object, a.BaseCurve.object, a.QuoteCurve.object, a.BaseDayCounter.object, a.QuoteDayCounter.object, a.BaseCurrency.object, a.QuoteCurrency.object, a.Settings.object); err != nil {
			return err
		}
		cfg := C.ItofinXccyBasisSwapConfig{base_schedule: C.uint64_t(a.BaseSchedule.id), quote_schedule: C.uint64_t(a.QuoteSchedule.id), base_index: C.uint64_t(a.BaseIndex.id), quote_index: C.uint64_t(a.QuoteIndex.id), base_curve: C.uint64_t(a.BaseCurve.id), quote_curve: C.uint64_t(a.QuoteCurve.id), base_day_counter: C.uint64_t(a.BaseDayCounter.id), quote_day_counter: C.uint64_t(a.QuoteDayCounter.id), base_currency: C.uint64_t(a.BaseCurrency.id), quote_currency: C.uint64_t(a.QuoteCurrency.id), base_notional: C.double(a.BaseNotional), quote_notional: C.double(a.QuoteNotional), quote_spread: C.double(a.QuoteSpread), spot: C.double(a.Spot), settings: C.uint64_t(a.Settings.id)}
		if a.PaymentConvention != nil {
			cfg.has_payment_convention = 1
			cfg.payment_convention = C.int32_t(*a.PaymentConvention)
		}
		if a.Valuation != nil {
			cfg.has_valuation_dates = 1
			cfg.settlement_date = C.int32_t(a.Valuation.SettlementDate.Serial())
			cfg.npv_date = C.int32_t(a.Valuation.NPVDate.Serial())
			if a.Valuation.IncludeSettlementFlows {
				cfg.include_settlement_flows = 1
			}
		}
		if a.SpotSettlementDate != nil {
			cfg.has_spot_settlement_date = 1
			cfg.spot_settlement_date = C.int32_t(a.SpotSettlementDate.Serial())
		}
		var e C.ItofinError
		return ffiError(C.itofin_xccy_basis_swap_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &XccyBasisSwap{object{s, uint64(id)}}, nil
}
func (v *XccyBasisSwap) value(query int32) (float64, error) {
	if v == nil {
		return 0, errNilArgument("cross-currency swap")
	}
	var out C.double
	err := v.session.invoke(func() error {
		if err := sameSession(v.session, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_xccy_basis_swap_value(v.session.ctx, C.uint64_t(v.id), C.int32_t(query), &out, &e), &e)
	})
	return float64(out), err
}

// BaseLegPV returns contractual base-currency leg PV, before its paid-leg sign.
func (v *XccyBasisSwap) BaseLegPV() (float64, error) { return v.value(0) }

// QuoteLegPV returns contractual quote-currency leg PV, before its received-leg sign.
func (v *XccyBasisSwap) QuoteLegPV() (float64, error) { return v.value(1) }

// QuoteLegBPS returns quote-currency spread sensitivity per basis point, excluding principal.
func (v *XccyBasisSwap) QuoteLegBPS() (float64, error) { return v.value(2) }

// NPV returns signed value in base currency at the selected NPV date.
func (v *XccyBasisSwap) NPV() (float64, error) { return v.value(3) }

// NPVInQuote returns signed value in quote currency at the selected NPV date.
func (v *XccyBasisSwap) NPVInQuote() (float64, error) { return v.value(4) }

// FairQuoteBasisSpread returns an absolute quote spread, not a spread adjustment.
func (v *XccyBasisSwap) FairQuoteBasisSpread() (float64, error) { return v.value(5) }

// Spot returns the immutable quote-currency units per one base unit.
func (v *XccyBasisSwap) Spot() (float64, error) { return v.value(6) }
func (v *XccyBasisSwap) currency(side int32) (*Currency, error) {
	if v == nil {
		return nil, errNilArgument("cross-currency swap")
	}
	var id C.uint64_t
	err := v.session.invoke(func() error {
		if err := sameSession(v.session, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_xccy_basis_swap_currency(v.session.ctx, C.uint64_t(v.id), C.int32_t(side), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Currency{object{v.session, uint64(id)}}, nil
}

// BaseCurrency returns an independently owned currency copy. Close the returned handle.
func (v *XccyBasisSwap) BaseCurrency() (*Currency, error) { return v.currency(0) }

// QuoteCurrency returns an independently owned currency copy. Close the returned handle.
func (v *XccyBasisSwap) QuoteCurrency() (*Currency, error) { return v.currency(1) }
