package itofin

/*
#include "itofin.h"
*/
import "C"
import "unsafe"

type RateHelper struct{ object }
type DepositRateHelper = RateHelper
type SwapRateHelper = RateHelper
type FraRateHelper = RateHelper
type FuturesRateHelper = RateHelper
type OISRateHelper = RateHelper
type FixedRateBondHelper = RateHelper

type Pillar int32

const (
	MaturityDate Pillar = iota
	LastRelevantDate
)

type FuturesType int32

const (
	Imm FuturesType = iota
	Asx
	Custom
)

type RateAveraging int32

const (
	SimpleAveraging RateAveraging = iota
	CompoundAveraging
)

type BondPriceType int32

const (
	Clean BondPriceType = iota
	Dirty
)

func helperResult(s *Session, id C.uint64_t, err error) (*RateHelper, error) {
	if err != nil {
		return nil, err
	}
	return &RateHelper{object{s, uint64(id)}}, nil
}
func (s *Session) NewDepositRateHelper(q *SimpleQuote, index *IborIndex) (*RateHelper, error) {
	if q == nil || index == nil {
		return nil, errNilArgument("deposit input")
	}
	if err := sameSession(s, q.object, index.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_deposit_helper_new(s.ctx, C.uint64_t(q.id), 0, false, C.uint64_t(index.id), &id, &e), &e)
	})
	return helperResult(s, id, err)
}
func (s *Session) NewDepositRateHelperFromRate(rate float64, index *IborIndex) (*RateHelper, error) {
	if index == nil {
		return nil, errNilArgument("index")
	}
	if err := sameSession(s, index.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_deposit_helper_new(s.ctx, 0, C.double(rate), true, C.uint64_t(index.id), &id, &e), &e)
	})
	return helperResult(s, id, err)
}

type SwapRateHelperConfig struct {
	Quote           *SimpleQuote
	Tenor           Period
	Calendar        *Calendar
	FixedFrequency  Frequency
	FixedConvention BusinessDayConvention
	FixedDayCount   *DayCounter
	IborIndex       *IborIndex
}

func (s *Session) NewSwapRateHelper(cfg SwapRateHelperConfig) (*RateHelper, error) {
	if cfg.Quote == nil || cfg.Calendar == nil || cfg.FixedDayCount == nil || cfg.IborIndex == nil {
		return nil, errNilArgument("swap helper input")
	}
	if err := sameSession(s, cfg.Quote.object, cfg.Calendar.object, cfg.FixedDayCount.object, cfg.IborIndex.object); err != nil {
		return nil, err
	}
	a := C.ItofinSwapHelperConfig{quote: C.uint64_t(cfg.Quote.id), tenor_length: C.int32_t(cfg.Tenor.Length), tenor_unit: C.int32_t(cfg.Tenor.Unit), calendar: C.uint64_t(cfg.Calendar.id), frequency: C.int32_t(cfg.FixedFrequency), convention: C.int32_t(cfg.FixedConvention), day_counter: C.uint64_t(cfg.FixedDayCount.id), index: C.uint64_t(cfg.IborIndex.id)}
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_swap_helper_new(s.ctx, &a, &id, &e), &e) })
	return helperResult(s, id, err)
}

type FraRateHelperConfig struct {
	Quote              *SimpleQuote
	Rate               float64 // FromRate constructor only.
	PeriodToStart      Period
	MonthsToStart      uint32 // FromMonths constructor only.
	StartDate, EndDate Date   // FromDates constructor only.
	Index              *IborIndex
	UseIndexedCoupon   bool
	Pillar             Pillar
}

// DefaultFraRateHelperConfig selects Python's defaults for coupon mode and pillar.
func DefaultFraRateHelperConfig() FraRateHelperConfig {
	return FraRateHelperConfig{UseIndexedCoupon: true, Pillar: LastRelevantDate}
}
func (s *Session) fraHelper(cfg FraRateHelperConfig, mode int) (*RateHelper, error) {
	if cfg.Index == nil {
		return nil, errNilArgument("index")
	}
	if err := sameSession(s, cfg.Index.object); err != nil {
		return nil, err
	}
	var q uint64
	if mode != 1 {
		if cfg.Quote == nil {
			return nil, errNilArgument("quote")
		}
		if err := sameSession(s, cfg.Quote.object); err != nil {
			return nil, err
		}
		q = cfg.Quote.id
	}
	a := C.ItofinFraHelperConfig{quote: C.uint64_t(q), rate: C.double(cfg.Rate), start_length: C.int32_t(cfg.PeriodToStart.Length), start_unit: C.int32_t(cfg.PeriodToStart.Unit), months: C.uint32_t(cfg.MonthsToStart), start_date: C.int32_t(cfg.StartDate.Serial()), end_date: C.int32_t(cfg.EndDate.Serial()), index: C.uint64_t(cfg.Index.id), indexed: C.bool(cfg.UseIndexedCoupon), pillar: C.int32_t(cfg.Pillar)}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_fra_helper_new(s.ctx, C.int32_t(mode), &a, &id, &e), &e)
	})
	return helperResult(s, id, err)
}
func (s *Session) NewFraRateHelper(c FraRateHelperConfig) (*RateHelper, error) {
	return s.fraHelper(c, 0)
}
func (s *Session) NewFraRateHelperFromRate(c FraRateHelperConfig) (*RateHelper, error) {
	return s.fraHelper(c, 1)
}
func (s *Session) NewFraRateHelperFromMonths(c FraRateHelperConfig) (*RateHelper, error) {
	return s.fraHelper(c, 2)
}
func (s *Session) NewFraRateHelperFromDates(c FraRateHelperConfig) (*RateHelper, error) {
	return s.fraHelper(c, 3)
}

type FuturesRateHelperConfig struct {
	DoNotObserveConvexity bool
	Price                 *SimpleQuote
	IborStartDate         Date
	IborEndDate           *Date // FromEndDate only; nil follows the core IMM/ASX rule.
	LengthInMonths        uint32
	Calendar              *Calendar
	Convention            BusinessDayConvention
	EndOfMonth            bool
	DayCounter            *DayCounter
	ConvexityAdjustment   *SimpleQuote
	FuturesType           FuturesType
	Index                 *IborIndex // FromIndex only.
}

func (s *Session) futuresHelper(cfg FuturesRateHelperConfig, mode int) (*RateHelper, error) {
	if cfg.Price == nil {
		return nil, errNilArgument("price")
	}
	if err := sameSession(s, cfg.Price.object); err != nil {
		return nil, err
	}
	a := C.ItofinFuturesHelperConfig{price: C.uint64_t(cfg.Price.id), start_date: C.int32_t(cfg.IborStartDate.Serial()), months: C.uint32_t(cfg.LengthInMonths), convention: C.int32_t(cfg.Convention), end_of_month: C.bool(cfg.EndOfMonth), futures_type: C.int32_t(cfg.FuturesType)}
	if cfg.IborEndDate != nil {
		a.end_date = C.int32_t(cfg.IborEndDate.Serial())
		a.has_end_date = true
	}
	if cfg.ConvexityAdjustment != nil {
		if err := sameSession(s, cfg.ConvexityAdjustment.object); err != nil {
			return nil, err
		}
		a.convexity = C.uint64_t(cfg.ConvexityAdjustment.id)
	}
	if mode == 2 {
		if cfg.Index == nil {
			return nil, errNilArgument("index")
		}
		if err := sameSession(s, cfg.Index.object); err != nil {
			return nil, err
		}
		a.index = C.uint64_t(cfg.Index.id)
	} else {
		if cfg.DayCounter == nil {
			return nil, errNilArgument("day counter")
		}
		if err := sameSession(s, cfg.DayCounter.object); err != nil {
			return nil, err
		}
		a.day_counter = C.uint64_t(cfg.DayCounter.id)
	}
	if mode == 0 {
		if cfg.Calendar == nil {
			return nil, errNilArgument("calendar")
		}
		if err := sameSession(s, cfg.Calendar.object); err != nil {
			return nil, err
		}
		a.calendar = C.uint64_t(cfg.Calendar.id)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_futures_helper_new_with_observation(s.ctx, C.int32_t(mode), &a, C.bool(!cfg.DoNotObserveConvexity), &id, &e), &e)
	})
	return helperResult(s, id, err)
}
func (s *Session) NewFuturesRateHelper(c FuturesRateHelperConfig) (*RateHelper, error) {
	return s.futuresHelper(c, 0)
}
func (s *Session) NewFuturesRateHelperFromEndDate(c FuturesRateHelperConfig) (*RateHelper, error) {
	return s.futuresHelper(c, 1)
}
func (s *Session) NewFuturesRateHelperFromIndex(c FuturesRateHelperConfig) (*RateHelper, error) {
	return s.futuresHelper(c, 2)
}
func (h *RateHelper) value(query int) (float64, error) {
	var v C.double
	err := h.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_helper_value(h.session.ctx, C.uint64_t(h.id), C.int32_t(query), &v, &e), &e)
	})
	return float64(v), err
}
func (h *RateHelper) ImpliedQuote() (float64, error)        { return h.value(0) }
func (h *RateHelper) QuoteError() (float64, error)          { return h.value(1) }
func (h *RateHelper) QuoteValue() (float64, error)          { return h.value(2) }
func (h *RateHelper) ConvexityAdjustment() (float64, error) { return h.value(3) }
func (h *RateHelper) date(query int) (Date, error) {
	var v C.int32_t
	err := h.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_helper_date(h.session.ctx, C.uint64_t(h.id), C.int32_t(query), &v, &e), &e)
	})
	return Date{serial: int32(v)}, err
}
func (h *RateHelper) MaturityDate() (Date, error)       { return h.date(0) }
func (h *RateHelper) PillarDate() (Date, error)         { return h.date(1) }
func (h *RateHelper) EarliestDate() (Date, error)       { return h.date(2) }
func (h *RateHelper) LatestDate() (Date, error)         { return h.date(3) }
func (h *RateHelper) LatestRelevantDate() (Date, error) { return h.date(4) }

type OISRateHelperConfig struct {
	SettlementDays    uint32
	Tenor             Period
	Quote             *SimpleQuote
	OvernightIndex    *OvernightIndex
	PaymentLag        int32
	PaymentConvention BusinessDayConvention
	PaymentFrequency  Frequency
	ForwardStart      Period
	Settings          *Settings
	DiscountingCurve  *YieldTermStructure
	OvernightSpread   *SimpleQuote
	Pillar            Pillar
	AveragingMethod   RateAveraging
}

func DefaultOISRateHelperConfig() OISRateHelperConfig {
	return OISRateHelperConfig{Pillar: LastRelevantDate, AveragingMethod: CompoundAveraging}
}
func (s *Session) NewOISRateHelper(cfg OISRateHelperConfig) (*RateHelper, error) {
	if cfg.Quote == nil || cfg.OvernightIndex == nil || cfg.Settings == nil {
		return nil, errNilArgument("OIS helper input")
	}
	if err := sameSession(s, cfg.Quote.object, cfg.OvernightIndex.object, cfg.Settings.object); err != nil {
		return nil, err
	}
	a := C.ItofinOisHelperConfig{settlement_days: C.uint32_t(cfg.SettlementDays), tenor_length: C.int32_t(cfg.Tenor.Length), tenor_unit: C.int32_t(cfg.Tenor.Unit), quote: C.uint64_t(cfg.Quote.id), index: C.uint64_t(cfg.OvernightIndex.id), payment_lag: C.int32_t(cfg.PaymentLag), convention: C.int32_t(cfg.PaymentConvention), frequency: C.int32_t(cfg.PaymentFrequency), forward_length: C.int32_t(cfg.ForwardStart.Length), forward_unit: C.int32_t(cfg.ForwardStart.Unit), settings: C.uint64_t(cfg.Settings.id), pillar: C.int32_t(cfg.Pillar), averaging: C.int32_t(cfg.AveragingMethod)}
	if cfg.DiscountingCurve != nil {
		if err := sameSession(s, cfg.DiscountingCurve.object); err != nil {
			return nil, err
		}
		a.discounting = C.uint64_t(cfg.DiscountingCurve.id)
	}
	if cfg.OvernightSpread != nil {
		if err := sameSession(s, cfg.OvernightSpread.object); err != nil {
			return nil, err
		}
		a.spread = C.uint64_t(cfg.OvernightSpread.id)
	}
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_ois_helper_new(s.ctx, &a, &id, &e), &e) })
	return helperResult(s, id, err)
}

type FixedRateBondHelperConfig struct {
	Price             *SimpleQuote
	SettlementDays    uint32
	FaceAmount        float64
	Schedule          *Schedule
	Coupons           []float64
	DayCounter        *DayCounter
	PaymentConvention BusinessDayConvention
	Redemption        float64
	PriceType         BondPriceType
	Settings          *Settings
	IssueDate         *Date
}

func (s *Session) NewFixedRateBondHelper(cfg FixedRateBondHelperConfig) (*RateHelper, error) {
	if cfg.Price == nil || cfg.Schedule == nil || cfg.DayCounter == nil || cfg.Settings == nil {
		return nil, errNilArgument("bond helper input")
	}
	if err := sameSession(s, cfg.Price.object, cfg.Schedule.object, cfg.DayCounter.object, cfg.Settings.object); err != nil {
		return nil, err
	}
	a := C.ItofinBondHelperConfig{price: C.uint64_t(cfg.Price.id), settlement_days: C.uint32_t(cfg.SettlementDays), face_amount: C.double(cfg.FaceAmount), schedule: C.uint64_t(cfg.Schedule.id), day_counter: C.uint64_t(cfg.DayCounter.id), convention: C.int32_t(cfg.PaymentConvention), redemption: C.double(cfg.Redemption), price_type: C.int32_t(cfg.PriceType), settings: C.uint64_t(cfg.Settings.id)}
	if cfg.IssueDate != nil {
		a.issue_date = C.int32_t(cfg.IssueDate.Serial())
		a.has_issue_date = true
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bond_helper_new(s.ctx, &a, (*C.double)(unsafe.Pointer(unsafe.SliceData(cfg.Coupons))), C.size_t(len(cfg.Coupons)), &id, &e), &e)
	})
	return helperResult(s, id, err)
}
