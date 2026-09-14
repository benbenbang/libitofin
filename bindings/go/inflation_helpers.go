package itofin

// #include "itofin.h"
import "C"

type inflationHelper struct {
	object
	kind int32
}
type ZeroInflationHelper struct{ inflationHelper }
type YoYInflationHelper struct{ inflationHelper }
type ZeroCouponInflationSwapHelper = ZeroInflationHelper
type YearOnYearInflationSwapHelper = YoYInflationHelper
type InflationHelperConfig struct {
	Quote                    *SimpleQuote
	SwapObservationLag       Period
	Maturity                 Date
	Calendar                 *Calendar
	PaymentConvention        BusinessDayConvention
	DayCounter               *DayCounter
	ObservationInterpolation CpiInterpolationType
	Settings                 *Settings
	// Nil selects LastRelevantDate, matching Python.
	Pillar *Pillar
}

func (s *Session) inflationHelperNew(a InflationHelperConfig, index object, discount *YieldTermStructure, kind int32) (inflationHelper, error) {
	if a.Quote == nil || a.Calendar == nil || a.DayCounter == nil || a.Settings == nil {
		return inflationHelper{}, errNilArgument("inflation helper argument")
	}
	args := []object{a.Quote.object, a.Calendar.object, a.DayCounter.object, a.Settings.object, index}
	cfg := C.ItofinInflationHelperConfig{quote: C.uint64_t(a.Quote.id), lag_length: C.int32_t(a.SwapObservationLag.Length), lag_unit: C.int32_t(a.SwapObservationLag.Unit), maturity: C.int32_t(a.Maturity.Serial()), calendar: C.uint64_t(a.Calendar.id), convention: C.int32_t(a.PaymentConvention), day_counter: C.uint64_t(a.DayCounter.id), index: C.uint64_t(index.id), interpolation: C.int32_t(a.ObservationInterpolation), settings: C.uint64_t(a.Settings.id), pillar: C.int32_t(creditOptional(a.Pillar, LastRelevantDate))}
	if discount != nil {
		cfg.discount = C.uint64_t(discount.id)
		args = append(args, discount.object)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, args...); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_inflation_helper_new(s.ctx, &cfg, C.int32_t(kind), &id, &e), &e)
	})
	return inflationHelper{object{s, uint64(id)}, kind}, err
}
func (s *Session) NewZeroCouponInflationSwapHelper(a InflationHelperConfig, index *ZeroInflationIndex) (*ZeroCouponInflationSwapHelper, error) {
	if index == nil {
		return nil, errNilArgument("index")
	}
	h, e := s.inflationHelperNew(a, index.object, nil, 0)
	if e != nil {
		return nil, e
	}
	return &ZeroCouponInflationSwapHelper{h}, nil
}
func (s *Session) NewYearOnYearInflationSwapHelper(a InflationHelperConfig, index *YoYInflationIndex, nominalTermStructure *YieldTermStructure) (*YearOnYearInflationSwapHelper, error) {
	if index == nil || nominalTermStructure == nil {
		return nil, errNilArgument("index or nominal term structure")
	}
	h, e := s.inflationHelperNew(a, index.object, nominalTermStructure, 1)
	if e != nil {
		return nil, e
	}
	return &YearOnYearInflationSwapHelper{h}, nil
}
func (h inflationHelper) date(query int32) (Date, error) {
	var d C.int32_t
	err := h.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_inflation_helper_date(h.session.ctx, C.uint64_t(h.id), C.int32_t(h.kind), C.int32_t(query), &d, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(d))
}
func (h inflationHelper) PillarDate() (Date, error) { return h.date(0) }
func (h inflationHelper) LatestDate() (Date, error) { return h.date(1) }
func (h *ZeroInflationHelper) InflationFixingDate() (Date, error) {
	if h == nil {
		return Date{}, errNilArgument("helper")
	}
	return h.date(2)
}

// DefaultInflationHelperConfig preserves the Python default pillar convention.
func DefaultInflationHelperConfig() InflationHelperConfig {
	return InflationHelperConfig{}
}
