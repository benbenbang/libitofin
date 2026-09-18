package itofin

// #include "itofin.h"
import "C"
import "unsafe"

type DefaultProbabilityTermStructure struct{ object }
type FlatHazardRate = DefaultProbabilityTermStructure
type InterpolatedHazardRateCurve = DefaultProbabilityTermStructure
type PiecewiseDefaultCurve = DefaultProbabilityTermStructure

type FlatHazardConfig struct {
	ReferenceDate  Date
	SettlementDays uint32
	Calendar       *Calendar
	Quote          *SimpleQuote
	Rate           float64
	DayCounter     *DayCounter
	Settings       *Settings
}

// NewFlatHazardRate supports fixed/moving dates and quote/constant rates.
func (s *Session) NewFlatHazardRate(a FlatHazardConfig) (*FlatHazardRate, error) {
	if a.DayCounter == nil {
		return nil, errNilArgument("credit argument")
	}
	args := []object{a.DayCounter.object}
	c := C.ItofinFlatHazardConfig{reference_date: C.int32_t(a.ReferenceDate.Serial()), settlement_days: C.uint32_t(a.SettlementDays), rate: C.double(a.Rate), day_counter: C.uint64_t(a.DayCounter.id)}
	if a.Quote != nil {
		args = append(args, a.Quote.object)
		c.quote = C.uint64_t(a.Quote.id)
	}
	if a.Settings != nil {
		if a.Calendar == nil {
			return nil, errNilArgument("credit argument")
		}
		args = append(args, a.Settings.object, a.Calendar.object)
		c.settings = C.uint64_t(a.Settings.id)
		c.calendar = C.uint64_t(a.Calendar.id)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, args...); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_flat_hazard_new(s.ctx, &c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FlatHazardRate{object{s, uint64(id)}}, nil
}
func (s *Session) NewInterpolatedHazardRateCurve(dates []Date, rates []float64, dc *DayCounter) (*InterpolatedHazardRateCurve, error) {
	if dc == nil {
		return nil, errNilArgument("credit argument")
	}
	if len(dates) != len(rates) {
		return nil, creditArgumentError("dates/rates length mismatch")
	}
	ds := make([]C.int32_t, len(dates))
	for i, d := range dates {
		ds[i] = C.int32_t(d.Serial())
	}
	var dp *C.int32_t
	var rp *C.double
	if len(ds) > 0 {
		dp = &ds[0]
		rp = (*C.double)(unsafe.Pointer(&rates[0]))
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, dc.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_interpolated_hazard_new(s.ctx, dp, rp, C.size_t(len(ds)), C.uint64_t(dc.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &InterpolatedHazardRateCurve{object{s, uint64(id)}}, nil
}
func (s *Session) NewPiecewiseDefaultCurve(reference Date, helpers []*DefaultProbabilityHelper, dc *DayCounter) (*PiecewiseDefaultCurve, error) {
	if dc == nil {
		return nil, errNilArgument("credit argument")
	}
	args := []object{dc.object}
	ids := make([]C.uint64_t, len(helpers))
	for i, h := range helpers {
		if h == nil {
			return nil, errNilArgument("credit argument")
		}
		args = append(args, h.object)
		ids[i] = C.uint64_t(h.id)
	}
	var ptr *C.uint64_t
	if len(ids) > 0 {
		ptr = &ids[0]
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, args...); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_piecewise_default_new(s.ctx, C.int32_t(reference.Serial()), ptr, C.size_t(len(ids)), C.uint64_t(dc.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &PiecewiseDefaultCurve{object{s, uint64(id)}}, nil
}
func (c *DefaultProbabilityTermStructure) value(kind int, t float64, date Date, useDate, extrapolate bool) (float64, error) {
	if c == nil {
		return 0, errNilArgument("credit argument")
	}
	var out C.double
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_default_curve_value(c.session.ctx, C.uint64_t(c.id), C.int32_t(kind), C.double(t), C.int32_t(date.Serial()), creditBool(useDate), creditBool(extrapolate), &out, &e), &e)
	})
	return float64(out), err
}
func (c *DefaultProbabilityTermStructure) SurvivalProbability(t float64, extrapolate bool) (float64, error) {
	return c.value(0, t, Date{}, false, extrapolate)
}
func (c *DefaultProbabilityTermStructure) DefaultProbability(t float64, extrapolate bool) (float64, error) {
	return c.value(1, t, Date{}, false, extrapolate)
}
func (c *DefaultProbabilityTermStructure) DefaultDensity(t float64, extrapolate bool) (float64, error) {
	return c.value(2, t, Date{}, false, extrapolate)
}
func (c *DefaultProbabilityTermStructure) HazardRate(t float64, extrapolate bool) (float64, error) {
	return c.value(3, t, Date{}, false, extrapolate)
}
func (c *DefaultProbabilityTermStructure) SurvivalProbabilityDate(d Date, extrapolate bool) (float64, error) {
	return c.value(0, 0, d, true, extrapolate)
}
func (c *DefaultProbabilityTermStructure) DefaultProbabilityDate(d Date, extrapolate bool) (float64, error) {
	return c.value(1, 0, d, true, extrapolate)
}
func (c *DefaultProbabilityTermStructure) DefaultDensityDate(d Date, extrapolate bool) (float64, error) {
	return c.value(2, 0, d, true, extrapolate)
}
func (c *DefaultProbabilityTermStructure) HazardRateDate(d Date, extrapolate bool) (float64, error) {
	return c.value(3, 0, d, true, extrapolate)
}

type CreditNode struct {
	Date       Date
	Time, Rate float64
}

func (c *DefaultProbabilityTermStructure) Nodes() ([]CreditNode, error) {
	if c == nil {
		return nil, errNilArgument("credit argument")
	}
	var result []CreditNode
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var e C.ItofinError
		var n C.size_t
		if err := ffiError(C.itofin_default_curve_nodes(c.session.ctx, C.uint64_t(c.id), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		if n == 0 {
			return nil
		}
		buf := make([]C.ItofinCreditNode, int(n))
		if err := ffiError(C.itofin_default_curve_nodes(c.session.ctx, C.uint64_t(c.id), &buf[0], n, &n, &e), &e); err != nil {
			return err
		}
		result = make([]CreditNode, len(buf))
		for i, v := range buf {
			d, err := DateFromSerial(int32(v.date))
			if err != nil {
				return err
			}
			result[i] = CreditNode{d, float64(v.time), float64(v.rate)}
		}
		return nil
	})
	return result, err
}
func (c *DefaultProbabilityTermStructure) Dates() ([]Date, error) {
	n, e := c.Nodes()
	if e != nil {
		return nil, e
	}
	v := make([]Date, len(n))
	for i, x := range n {
		v[i] = x.Date
	}
	return v, nil
}
func (c *DefaultProbabilityTermStructure) Times() ([]float64, error) {
	n, e := c.Nodes()
	if e != nil {
		return nil, e
	}
	v := make([]float64, len(n))
	for i, x := range n {
		v[i] = x.Time
	}
	return v, nil
}
func (c *DefaultProbabilityTermStructure) Data() ([]float64, error) {
	n, e := c.Nodes()
	if e != nil {
		return nil, e
	}
	v := make([]float64, len(n))
	for i, x := range n {
		v[i] = x.Rate
	}
	return v, nil
}
func (c *DefaultProbabilityTermStructure) HazardRates() ([]float64, error) { return c.Data() }
func (c *DefaultProbabilityTermStructure) Calculate() error {
	if c == nil {
		return errNilArgument("credit argument")
	}
	return c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_default_curve_calculate(c.session.ctx, C.uint64_t(c.id), &e), &e)
	})
}
func creditBool(v bool) C.int32_t {
	if v {
		return 1
	}
	return 0
}

type creditArgumentError string

func (e creditArgumentError) Error() string { return string(e) }
