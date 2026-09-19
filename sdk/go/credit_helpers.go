package itofin

// #include "itofin.h"
import "C"

type CdsHelperTerms struct {
	Model                PricingModel
	SettlesAccrual       *bool
	PaysAtDefaultTime    *bool
	StartDate            *Date
	LastPeriodDayCounter *DayCounter
	RebatesAccrual       *bool
}

type UpfrontCdsHelper = DefaultProbabilityHelper

type UpfrontCdsHelperConfig struct {
	Upfront               *SimpleQuote
	RunningSpread         float64
	Tenor                 Period
	SettlementDays        int32
	Calendar              *Calendar
	Frequency             Frequency
	PaymentConvention     BusinessDayConvention
	Rule                  DateGeneration
	DayCounter            *DayCounter
	RecoveryRate          float64
	DiscountCurve         *YieldTermStructure
	Settings              *Settings
	UpfrontSettlementDays *uint32
	Terms                 CdsHelperTerms
}

func (s *Session) NewSpreadCdsHelperWithTerms(a SpreadCdsHelperConfig, terms CdsHelperTerms) (*SpreadCdsHelper, error) {
	return s.cdsHelperWithTerms(a, terms, 0, 0, 0)
}

func (s *Session) NewUpfrontCdsHelper(a UpfrontCdsHelperConfig) (*UpfrontCdsHelper, error) {
	base := SpreadCdsHelperConfig{
		RunningSpread: a.Upfront, Tenor: a.Tenor, SettlementDays: a.SettlementDays,
		Calendar: a.Calendar, Frequency: a.Frequency, PaymentConvention: a.PaymentConvention,
		Rule: a.Rule, DayCounter: a.DayCounter, RecoveryRate: a.RecoveryRate,
		DiscountCurve: a.DiscountCurve, Settings: a.Settings,
	}
	days := uint32(3)
	if a.UpfrontSettlementDays != nil {
		days = *a.UpfrontSettlementDays
	}
	return s.cdsHelperWithTerms(base, a.Terms, 1, a.RunningSpread, days)
}

func (s *Session) cdsHelperWithTerms(a SpreadCdsHelperConfig, terms CdsHelperTerms, kind int, runningSpread float64, upfrontDays uint32) (*DefaultProbabilityHelper, error) {
	if a.RunningSpread == nil || a.Calendar == nil || a.DayCounter == nil || a.DiscountCurve == nil || a.Settings == nil {
		return nil, errNilArgument("credit argument")
	}
	cfg := C.ItofinSpreadCdsConfig{quote: C.uint64_t(a.RunningSpread.id), tenor_length: C.int32_t(a.Tenor.Length), tenor_unit: C.int32_t(a.Tenor.Unit), settlement_days: C.int32_t(a.SettlementDays), calendar: C.uint64_t(a.Calendar.id), frequency: C.int32_t(a.Frequency), convention: C.int32_t(a.PaymentConvention), rule: C.int32_t(a.Rule), day_counter: C.uint64_t(a.DayCounter.id), recovery: C.double(a.RecoveryRate), discount: C.uint64_t(a.DiscountCurve.id), settings: C.uint64_t(a.Settings.id)}
	ct := C.ItofinCdsHelperTerms{model: C.int32_t(terms.Model), settles_accrual: 1, pays_at_default_time: 1, rebates_accrual: 1}
	if terms.SettlesAccrual != nil {
		ct.settles_accrual = creditBool(*terms.SettlesAccrual)
	}
	if terms.PaysAtDefaultTime != nil {
		ct.pays_at_default_time = creditBool(*terms.PaysAtDefaultTime)
	}
	if terms.RebatesAccrual != nil {
		ct.rebates_accrual = creditBool(*terms.RebatesAccrual)
	}
	if terms.StartDate != nil {
		ct.start_date = C.int32_t(terms.StartDate.Serial())
	}
	objects := []object{a.RunningSpread.object, a.Calendar.object, a.DayCounter.object, a.DiscountCurve.object, a.Settings.object}
	if terms.LastPeriodDayCounter != nil {
		ct.last_period_day_counter = C.uint64_t(terms.LastPeriodDayCounter.id)
		objects = append(objects, terms.LastPeriodDayCounter.object)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_helper_with_terms(s.ctx, &cfg, &ct, C.int32_t(kind), C.double(runningSpread), C.uint32_t(upfrontDays), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &DefaultProbabilityHelper{object{s, uint64(id)}}, nil
}

func (h *DefaultProbabilityHelper) ImpliedQuote() (float64, error) {
	if h == nil {
		return 0, errNilArgument("credit helper")
	}
	var out C.double
	err := h.session.invoke(func() error {
		if err := sameSession(h.session, h.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_helper_implied_quote(h.session.ctx, C.uint64_t(h.id), &out, &e), &e)
	})
	return float64(out), err
}
