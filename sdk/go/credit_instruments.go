package itofin

// #include "itofin.h"
import "C"

type ProtectionSide int32

const (
	ProtectionBuyer ProtectionSide = iota
	ProtectionSeller
)

type PricingModel int32

const (
	Midpoint PricingModel = iota
	Isda
)

type NumericalFix int32

const (
	NoFix NumericalFix = iota
	Taylor
)

type AccrualBias int32

const (
	HalfDayBias AccrualBias = iota
	NoBias
)

type ForwardsInCouponPeriod int32

const (
	FlatForwards ForwardsInCouponPeriod = iota
	PiecewiseForwards
)

type DefaultProbabilityHelper struct{ object }
type SpreadCdsHelper = DefaultProbabilityHelper
type CreditDefaultSwap struct{ object }
type CdsEngine struct{ object }
type MidPointCdsEngine = CdsEngine
type IsdaCdsEngine = CdsEngine

type SpreadCdsHelperConfig struct {
	RunningSpread     *SimpleQuote
	Tenor             Period
	SettlementDays    int32
	Calendar          *Calendar
	Frequency         Frequency
	PaymentConvention BusinessDayConvention
	Rule              DateGeneration
	DayCounter        *DayCounter
	RecoveryRate      float64
	DiscountCurve     *YieldTermStructure
	Settings          *Settings
}

func (s *Session) NewSpreadCdsHelper(a SpreadCdsHelperConfig) (*SpreadCdsHelper, error) {
	if a.RunningSpread == nil || a.Calendar == nil || a.DayCounter == nil || a.DiscountCurve == nil || a.Settings == nil {
		return nil, errNilArgument("credit argument")
	}
	cfg := C.ItofinSpreadCdsConfig{quote: C.uint64_t(a.RunningSpread.id), tenor_length: C.int32_t(a.Tenor.Length), tenor_unit: C.int32_t(a.Tenor.Unit), settlement_days: C.int32_t(a.SettlementDays), calendar: C.uint64_t(a.Calendar.id), frequency: C.int32_t(a.Frequency), convention: C.int32_t(a.PaymentConvention), rule: C.int32_t(a.Rule), day_counter: C.uint64_t(a.DayCounter.id), recovery: C.double(a.RecoveryRate), discount: C.uint64_t(a.DiscountCurve.id), settings: C.uint64_t(a.Settings.id)}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, a.RunningSpread.object, a.Calendar.object, a.DayCounter.object, a.DiscountCurve.object, a.Settings.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_spread_cds_helper_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SpreadCdsHelper{object{s, uint64(id)}}, nil
}
func (h *DefaultProbabilityHelper) dates() (Date, Date, error) {
	if h == nil {
		return Date{}, Date{}, errNilArgument("credit argument")
	}
	var p, l C.int32_t
	err := h.session.invoke(func() error {
		if e := sameSession(h.session, h.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_default_helper_dates(h.session.ctx, C.uint64_t(h.id), &p, &l, &e), &e)
	})
	if err != nil {
		return Date{}, Date{}, err
	}
	pd, err := DateFromSerial(int32(p))
	if err != nil {
		return Date{}, Date{}, err
	}
	ld, err := DateFromSerial(int32(l))
	return pd, ld, err
}
func (h *DefaultProbabilityHelper) PillarDate() (Date, error) { p, _, e := h.dates(); return p, e }
func (h *DefaultProbabilityHelper) LatestDate() (Date, error) { _, l, e := h.dates(); return l, e }

type CdsEngineConfig struct {
	Probability *DefaultProbabilityTermStructure
	Discount    *YieldTermStructure
	Settings    *Settings
	Recovery    float64
	// Nil selects Taylor.
	NumericalFix *NumericalFix
	AccrualBias  AccrualBias
	// Nil selects PiecewiseForwards.
	Forwards *ForwardsInCouponPeriod
}

func (s *Session) cdsEngine(a CdsEngineConfig, kind int) (*CdsEngine, error) {
	if a.Probability == nil || a.Discount == nil || a.Settings == nil {
		return nil, errNilArgument("credit argument")
	}
	cfg := C.ItofinCdsEngineConfig{probability: C.uint64_t(a.Probability.id), discount: C.uint64_t(a.Discount.id), settings: C.uint64_t(a.Settings.id), recovery: C.double(a.Recovery), kind: C.int32_t(kind), numerical_fix: C.int32_t(creditOptional(a.NumericalFix, Taylor)), accrual_bias: C.int32_t(a.AccrualBias), forwards: C.int32_t(creditOptional(a.Forwards, PiecewiseForwards))}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, a.Probability.object, a.Discount.object, a.Settings.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_engine_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CdsEngine{object{s, uint64(id)}}, nil
}
func (s *Session) NewMidPointCdsEngine(a CdsEngineConfig) (*MidPointCdsEngine, error) {
	return s.cdsEngine(a, 0)
}

// DefaultIsdaCdsEngineConfig selects the Python/QuantLib fidelity defaults.
func DefaultIsdaCdsEngineConfig() CdsEngineConfig {
	return CdsEngineConfig{}
}
func (s *Session) NewIsdaCdsEngine(a CdsEngineConfig) (*IsdaCdsEngine, error) {
	return s.cdsEngine(a, 1)
}

type CdsConfig struct {
	Side              ProtectionSide
	Notional, Spread  float64
	Schedule          *Schedule
	PaymentConvention BusinessDayConvention
	DayCounter        *DayCounter
	Settings          *Settings
	ProtectionStart   *Date
	// Nil flags select true, matching Python with_terms defaults.
	SettlesAccrual, PaysAtDefaultTime, RebatesAccrual *bool
}

func DefaultCdsConfig() CdsConfig {
	return CdsConfig{}
}
func (s *Session) NewCreditDefaultSwap(a CdsConfig) (*CreditDefaultSwap, error) {
	if a.Schedule == nil || a.DayCounter == nil || a.Settings == nil {
		return nil, errNilArgument("credit argument")
	}
	cfg := C.ItofinCdsConfig{side: C.int32_t(a.Side), notional: C.double(a.Notional), spread: C.double(a.Spread), schedule: C.uint64_t(a.Schedule.id), convention: C.int32_t(a.PaymentConvention), day_counter: C.uint64_t(a.DayCounter.id), settings: C.uint64_t(a.Settings.id), settles_accrual: creditBool(creditOptional(a.SettlesAccrual, true)), pays_at_default_time: creditBool(creditOptional(a.PaysAtDefaultTime, true)), rebates_accrual: creditBool(creditOptional(a.RebatesAccrual, true))}
	if a.ProtectionStart != nil {
		cfg.protection_start = C.int32_t(a.ProtectionStart.Serial())
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, a.Schedule.object, a.DayCounter.object, a.Settings.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CreditDefaultSwap{object{s, uint64(id)}}, nil
}

type MakeCreditDefaultSwapConfig struct {
	TermDate      Date
	RunningSpread float64
	Settings      *Settings
	Nominal       *float64
	UpfrontRate   *float64
	Side          ProtectionSide
	TradeDate     *Date
}

func (s *Session) MakeCreditDefaultSwap(a MakeCreditDefaultSwapConfig) (*CreditDefaultSwap, error) {
	if a.Settings == nil {
		return nil, errNilArgument("credit argument")
	}
	cfg := C.ItofinMakeCdsConfig{term_date: C.int32_t(a.TermDate.Serial()), running_spread: C.double(a.RunningSpread), settings: C.uint64_t(a.Settings.id), nominal: 1, side: C.int32_t(a.Side)}
	if a.Nominal != nil {
		cfg.nominal = C.double(*a.Nominal)
	}
	if a.UpfrontRate != nil {
		cfg.upfront = C.double(*a.UpfrontRate)
		cfg.has_upfront = 1
	}
	if a.TradeDate != nil {
		cfg.trade_date = C.int32_t(a.TradeDate.Serial())
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, a.Settings.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_make_cds(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CreditDefaultSwap{object{s, uint64(id)}}, nil
}
func (c *CreditDefaultSwap) SetEngine(engine *CdsEngine) error {
	if c == nil || engine == nil {
		return errNilArgument("credit argument")
	}
	return c.session.invoke(func() error {
		if e := sameSession(c.session, c.object, engine.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_set_engine(c.session.ctx, C.uint64_t(c.id), C.uint64_t(engine.id), &e), &e)
	})
}
func (c *CreditDefaultSwap) SetIsdaEngine(engine *IsdaCdsEngine) error { return c.SetEngine(engine) }
func (c *CreditDefaultSwap) value(query int) (float64, error) {
	if c == nil {
		return 0, errNilArgument("credit argument")
	}
	var out C.double
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_value(c.session.ctx, C.uint64_t(c.id), C.int32_t(query), &out, &e), &e)
	})
	return float64(out), err
}
func (c *CreditDefaultSwap) NPV() (float64, error)           { return c.value(0) }
func (c *CreditDefaultSwap) FairSpread() (float64, error)    { return c.value(1) }
func (c *CreditDefaultSwap) FairUpfront() (float64, error)   { return c.value(2) }
func (c *CreditDefaultSwap) Notional() (float64, error)      { return c.value(3) }
func (c *CreditDefaultSwap) CouponLegNPV() (float64, error)  { return c.value(4) }
func (c *CreditDefaultSwap) DefaultLegNPV() (float64, error) { return c.value(5) }
func (c *CreditDefaultSwap) Calculate() error                { _, e := c.value(6); return e }
func (c *CreditDefaultSwap) IsCalculated() (bool, error)     { v, e := c.value(7); return v != 0, e }

// Price attaches and values atomically relative to other session callers.
func (c *CreditDefaultSwap) Price(engine *MidPointCdsEngine) (float64, error) {
	if c == nil || engine == nil {
		return 0, errNilArgument("instrument or engine")
	}
	var out C.double
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object, engine.object); e != nil {
			return e
		}
		var e C.ItofinError
		if err := ffiError(C.itofin_cds_set_engine(c.session.ctx, C.uint64_t(c.id), C.uint64_t(engine.id), &e), &e); err != nil {
			return err
		}
		return ffiError(C.itofin_cds_value(c.session.ctx, C.uint64_t(c.id), 0, &out, &e), &e)
	})
	return float64(out), err
}
func (c *CreditDefaultSwap) ProtectionEndDate() (Date, error) {
	if c == nil {
		return Date{}, errNilArgument("credit argument")
	}
	var serial C.int32_t
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_protection_end_date(c.session.ctx, C.uint64_t(c.id), &serial, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(serial))
}

func (c *CreditDefaultSwap) Rebate() (amount *float64, date *Date, err error) {
	if c == nil {
		return nil, nil, errNilArgument("credit argument")
	}
	var has, serial C.int32_t
	var value C.double
	err = c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_rebate(c.session.ctx, C.uint64_t(c.id), &has, &value, &serial, &e), &e)
	})
	if err != nil || has == 0 {
		return nil, nil, err
	}
	v := float64(value)
	d, e := DateFromSerial(int32(serial))
	if e != nil {
		return nil, nil, e
	}
	return &v, &d, nil
}
func (c *CreditDefaultSwap) AccrualRebateAmount() (*float64, error) {
	v, _, e := c.Rebate()
	return v, e
}
func (c *CreditDefaultSwap) AccrualRebateDate() (*Date, error) { _, d, e := c.Rebate(); return d, e }
func (c *CreditDefaultSwap) ImpliedHazardRate(target float64, discount *YieldTermStructure, dc *DayCounter, recovery, accuracy float64, model PricingModel) (float64, error) {
	if c == nil || discount == nil || dc == nil {
		return 0, errNilArgument("credit argument")
	}
	var out C.double
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object, discount.object, dc.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cds_implied_hazard(c.session.ctx, C.uint64_t(c.id), C.double(target), C.uint64_t(discount.id), C.uint64_t(dc.id), C.double(recovery), C.double(accuracy), C.int32_t(model), &out, &e), &e)
	})
	return float64(out), err
}
func (c *CreditDefaultSwap) Results() (*Results, error) {
	if c == nil {
		return nil, errNilArgument("credit argument")
	}
	var result *Results
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var e C.ItofinError
		var id C.uint64_t
		if err := ffiError(C.itofin_cds_results(c.session.ctx, C.uint64_t(c.id), &id, &e), &e); err != nil {
			return err
		}
		var err error
		result, err = c.session.readResults(uint64(id))
		return err
	})
	return result, err
}

func creditOptional[T any](value *T, fallback T) T {
	if value != nil {
		return *value
	}
	return fallback
}
