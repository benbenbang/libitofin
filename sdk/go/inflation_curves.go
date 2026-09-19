package itofin

// #include "itofin.h"
import "C"
import "unsafe"

type inflationCurve struct {
	object
	kind int32
}
type ZeroInflationTermStructure struct{ inflationCurve }
type YoYInflationTermStructure struct{ inflationCurve }
type InterpolatedZeroInflationCurve = ZeroInflationTermStructure
type InterpolatedYoYInflationCurve = YoYInflationTermStructure
type PiecewiseZeroInflationCurve = ZeroInflationTermStructure
type PiecewiseYoYInflationCurve = YoYInflationTermStructure
type InflationCurveConfig struct {
	ReferenceDate, BaseDate Date
	BaseYoYRate             float64
	Frequency               Frequency
	DayCounter              *DayCounter
}

func (a InflationCurveConfig) native() C.ItofinInflationCurveConfig {
	return C.ItofinInflationCurveConfig{reference: C.int32_t(a.ReferenceDate.Serial()), base_date: C.int32_t(a.BaseDate.Serial()), base_rate: C.double(a.BaseYoYRate), frequency: C.int32_t(a.Frequency), day_counter: C.uint64_t(a.DayCounter.id)}
}
func (s *Session) inflationCurveNew(a InflationCurveConfig, dates []Date, rates []float64, kind int32) (inflationCurve, error) {
	if a.DayCounter == nil {
		return inflationCurve{}, errNilArgument("day counter")
	}
	if len(dates) != len(rates) {
		return inflationCurve{}, creditArgumentError("dates/rates length mismatch")
	}
	cfg := a.native()
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
		if e := sameSession(s, a.DayCounter.object); e != nil {
			return e
		}
		var e C.ItofinError
		var status C.int32_t
		if kind == 0 {
			status = C.itofin_zero_inflation_curve_new(s.ctx, &cfg, dp, rp, C.size_t(len(ds)), &id, &e)
		} else {
			status = C.itofin_yoy_inflation_curve_new(s.ctx, &cfg, dp, rp, C.size_t(len(ds)), &id, &e)
		}
		return ffiError(status, &e)
	})
	return inflationCurve{object{s, uint64(id)}, kind}, err
}
func (s *Session) NewInterpolatedZeroInflationCurve(a InflationCurveConfig, dates []Date, rates []float64) (*InterpolatedZeroInflationCurve, error) {
	c, e := s.inflationCurveNew(a, dates, rates, 0)
	if e != nil {
		return nil, e
	}
	return &InterpolatedZeroInflationCurve{c}, nil
}
func (s *Session) NewInterpolatedYoYInflationCurve(a InflationCurveConfig, dates []Date, rates []float64) (*InterpolatedYoYInflationCurve, error) {
	c, e := s.inflationCurveNew(a, dates, rates, 1)
	if e != nil {
		return nil, e
	}
	return &InterpolatedYoYInflationCurve{c}, nil
}
func (s *Session) piecewiseInflationNew(a InflationCurveConfig, helpers []object, kind int32) (inflationCurve, error) {
	if a.DayCounter == nil {
		return inflationCurve{}, errNilArgument("day counter")
	}
	cfg := a.native()
	args := append([]object{a.DayCounter.object}, helpers...)
	ids := make([]C.uint64_t, len(helpers))
	for i, h := range helpers {
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
		var status C.int32_t
		if kind == 0 {
			status = C.itofin_piecewise_zero_inflation_new(s.ctx, &cfg, ptr, C.size_t(len(ids)), &id, &e)
		} else {
			status = C.itofin_piecewise_yoy_inflation_new(s.ctx, &cfg, ptr, C.size_t(len(ids)), &id, &e)
		}
		return ffiError(status, &e)
	})
	return inflationCurve{object{s, uint64(id)}, kind}, err
}
func (s *Session) NewPiecewiseZeroInflationCurve(a InflationCurveConfig, helpers []*ZeroInflationHelper) (*PiecewiseZeroInflationCurve, error) {
	hs := make([]object, len(helpers))
	for i, h := range helpers {
		if h == nil {
			return nil, errNilArgument("helper")
		}
		hs[i] = h.object
	}
	c, e := s.piecewiseInflationNew(a, hs, 0)
	if e != nil {
		return nil, e
	}
	return &PiecewiseZeroInflationCurve{c}, nil
}
func (s *Session) NewPiecewiseZeroInflationCurveWithLastFixingDate(a InflationCurveConfig, index *ZeroInflationIndex, helpers []*ZeroInflationHelper, seasonality *MultiplicativePriceSeasonality) (*PiecewiseZeroInflationCurve, error) {
	if a.DayCounter == nil || index == nil {
		return nil, errNilArgument("day counter or inflation index")
	}
	args := []object{a.DayCounter.object, index.object}
	var season C.uint64_t
	if seasonality != nil {
		args = append(args, seasonality.object)
		season = C.uint64_t(seasonality.id)
	}
	ids := make([]C.uint64_t, len(helpers))
	for i, helper := range helpers {
		if helper == nil {
			return nil, errNilArgument("inflation helper")
		}
		args = append(args, helper.object)
		ids[i] = C.uint64_t(helper.id)
	}
	var ptr *C.uint64_t
	if len(ids) != 0 {
		ptr = &ids[0]
	}
	cfg := a.native()
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, args...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_piecewise_zero_inflation_last_fixing_new(s.ctx, &cfg, C.uint64_t(index.id), ptr, C.size_t(len(ids)), season, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &PiecewiseZeroInflationCurve{inflationCurve{object{s, uint64(id)}, 0}}, nil
}

func (s *Session) NewPiecewiseYoYInflationCurve(a InflationCurveConfig, helpers []*YoYInflationHelper) (*PiecewiseYoYInflationCurve, error) {
	hs := make([]object, len(helpers))
	for i, h := range helpers {
		if h == nil {
			return nil, errNilArgument("helper")
		}
		hs[i] = h.object
	}
	c, e := s.piecewiseInflationNew(a, hs, 1)
	if e != nil {
		return nil, e
	}
	return &PiecewiseYoYInflationCurve{c}, nil
}
func (c inflationCurve) value(query int32, t float64, date Date, extrapolate bool) (float64, error) {
	var v C.double
	err := c.session.invoke(func() error {
		var e C.ItofinError
		var status C.int32_t
		if c.kind == 0 {
			status = C.itofin_zero_inflation_curve_value(c.session.ctx, C.uint64_t(c.id), C.int32_t(query), C.double(t), C.int32_t(date.Serial()), creditBool(extrapolate), &v, &e)
		} else {
			status = C.itofin_yoy_inflation_curve_value(c.session.ctx, C.uint64_t(c.id), C.int32_t(query), C.double(t), C.int32_t(date.Serial()), creditBool(extrapolate), &v, &e)
		}
		return ffiError(status, &e)
	})
	return float64(v), err
}
func (c *ZeroInflationTermStructure) ZeroRate(t float64, extrapolate bool) (float64, error) {
	if c == nil {
		return 0, errNilArgument("curve")
	}
	return c.value(0, t, Date{}, extrapolate)
}
func (c *ZeroInflationTermStructure) ZeroRateDate(date Date, extrapolate bool) (float64, error) {
	if c == nil {
		return 0, errNilArgument("curve")
	}
	return c.value(1, 0, date, extrapolate)
}
func (c *YoYInflationTermStructure) YoYRate(t float64, extrapolate bool) (float64, error) {
	if c == nil {
		return 0, errNilArgument("curve")
	}
	return c.value(0, t, Date{}, extrapolate)
}
func (c *YoYInflationTermStructure) YoYRateDate(date Date, extrapolate bool) (float64, error) {
	if c == nil {
		return 0, errNilArgument("curve")
	}
	return c.value(1, 0, date, extrapolate)
}
func (c *YoYInflationTermStructure) BaseRate() (float64, error) {
	if c == nil {
		return 0, errNilArgument("curve")
	}
	return c.value(6, 0, Date{}, false)
}
func (c inflationCurve) BaseDate() (Date, error) {
	v, e := c.value(2, 0, Date{}, false)
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(int32(v))
}
func (c inflationCurve) Frequency() (Frequency, error) {
	v, e := c.value(3, 0, Date{}, false)
	return Frequency(v), e
}
func (c inflationCurve) HasSeasonality() (bool, error) {
	v, e := c.value(4, 0, Date{}, false)
	return v != 0, e
}
func (c inflationCurve) Calculate() error { _, e := c.value(5, 0, Date{}, false); return e }
func (c inflationCurve) SetSeasonality(seasonality *MultiplicativePriceSeasonality) error {
	var id C.uint64_t
	args := []object{c.object}
	if seasonality != nil {
		id = C.uint64_t(seasonality.id)
		args = append(args, seasonality.object)
	}
	return c.session.invoke(func() error {
		if e := sameSession(c.session, args...); e != nil {
			return e
		}
		var e C.ItofinError
		var status C.int32_t
		if c.kind == 0 {
			status = C.itofin_zero_inflation_set_seasonality(c.session.ctx, C.uint64_t(c.id), id, &e)
		} else {
			status = C.itofin_yoy_inflation_set_seasonality(c.session.ctx, C.uint64_t(c.id), id, &e)
		}
		return ffiError(status, &e)
	})
}

type InflationNode struct {
	Date       Date
	Time, Rate float64
}

func (c inflationCurve) Nodes() ([]InflationNode, error) {
	var nodes []InflationNode
	err := c.session.invoke(func() error {
		var e C.ItofinError
		var n C.size_t
		read := func(ptr *C.ItofinInflationNode, cap C.size_t) error {
			var status C.int32_t
			if c.kind == 0 {
				status = C.itofin_zero_inflation_nodes(c.session.ctx, C.uint64_t(c.id), ptr, cap, &n, &e)
			} else {
				status = C.itofin_yoy_inflation_nodes(c.session.ctx, C.uint64_t(c.id), ptr, cap, &n, &e)
			}
			return ffiError(status, &e)
		}
		if err := read(nil, 0); err != nil {
			return err
		}
		if n == 0 {
			return nil
		}
		buf := make([]C.ItofinInflationNode, int(n))
		if err := read(&buf[0], n); err != nil {
			return err
		}
		nodes = make([]InflationNode, len(buf))
		for i, v := range buf {
			d, err := DateFromSerial(int32(v.date))
			if err != nil {
				return err
			}
			nodes[i] = InflationNode{d, float64(v.time), float64(v.rate)}
		}
		return nil
	})
	return nodes, err
}
func (c inflationCurve) Dates() ([]Date, error) {
	n, e := c.Nodes()
	if e != nil {
		return nil, e
	}
	out := make([]Date, len(n))
	for i, v := range n {
		out[i] = v.Date
	}
	return out, nil
}
func (c inflationCurve) Times() ([]float64, error) {
	n, e := c.Nodes()
	if e != nil {
		return nil, e
	}
	out := make([]float64, len(n))
	for i, v := range n {
		out[i] = v.Time
	}
	return out, nil
}
