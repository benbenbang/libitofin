package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

// YieldTermStructure owns a native yield curve. Dependent native objects retain it.
type YieldTermStructure struct{ object }
type FlatForward = YieldTermStructure
type ZeroCurve = YieldTermStructure
type DiscountCurve = YieldTermStructure
type ForwardCurve = YieldTermStructure
type PiecewiseYieldCurve = YieldTermStructure
type PiecewiseLogLinearDiscount = YieldTermStructure
type PiecewiseLinearZero = YieldTermStructure
type PiecewiseCubicZero = YieldTermStructure
type PiecewiseLinearForward = YieldTermStructure
type PiecewiseConvexMonotoneForward = YieldTermStructure
type PiecewiseFlatForward = YieldTermStructure

func (s *Session) NewFlatForward(reference Date, rate float64, dc *DayCounter) (*YieldTermStructure, error) {
	if dc == nil {
		return nil, errNilArgument("day counter")
	}
	if err := sameSession(s, dc.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_flat_forward_new(s.ctx, C.int32_t(reference.Serial()), C.double(rate), C.uint64_t(dc.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YieldTermStructure{object{s, uint64(id)}}, nil
}

type NodeCurveConfig struct {
	Dates         []Date
	Values        []float64
	DayCounter    *DayCounter
	Calendar      *Calendar // Optional; used by DiscountCurve.
	Interpolation string    // Linear for ZeroCurve; LogLinear for DiscountCurve; Cubic accepted by both.
}

func (s *Session) nodeCurve(cfg NodeCurveConfig, kind int) (*YieldTermStructure, error) {
	if cfg.DayCounter == nil {
		return nil, errNilArgument("day counter")
	}
	if err := sameSession(s, cfg.DayCounter.object); err != nil {
		return nil, err
	}
	if len(cfg.Dates) != len(cfg.Values) {
		return nil, fmt.Errorf("dates and values must have equal lengths")
	}
	var cal uint64
	if cfg.Calendar != nil {
		if err := sameSession(s, cfg.Calendar.object); err != nil {
			return nil, err
		}
		cal = cfg.Calendar.id
	}
	ds := make([]C.int32_t, len(cfg.Dates))
	for i, d := range cfg.Dates {
		ds[i] = C.int32_t(d.Serial())
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_node_curve_new(s.ctx, C.int32_t(kind), (*C.int32_t)(unsafe.Pointer(unsafe.SliceData(ds))), (*C.double)(unsafe.Pointer(unsafe.SliceData(cfg.Values))), C.size_t(len(ds)), C.uint64_t(cfg.DayCounter.id), C.uint64_t(cal), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YieldTermStructure{object{s, uint64(id)}}, nil
}
func (s *Session) NewZeroCurve(cfg NodeCurveConfig) (*YieldTermStructure, error) {
	kind := 0
	switch cfg.Interpolation {
	case "", "Linear":
	case "Cubic":
		kind = 1
	default:
		return nil, fmt.Errorf("unknown zero interpolation %q", cfg.Interpolation)
	}
	return s.nodeCurve(cfg, kind)
}
func (s *Session) NewDiscountCurve(cfg NodeCurveConfig) (*YieldTermStructure, error) {
	kind := 2
	switch cfg.Interpolation {
	case "", "LogLinear":
	case "Cubic":
		kind = 3
	default:
		return nil, fmt.Errorf("unknown discount interpolation %q", cfg.Interpolation)
	}
	return s.nodeCurve(cfg, kind)
}
func (s *Session) NewForwardCurve(cfg NodeCurveConfig) (*YieldTermStructure, error) {
	if cfg.Interpolation != "" && cfg.Interpolation != "BackwardFlat" {
		return nil, fmt.Errorf("forward curve requires BackwardFlat interpolation")
	}
	return s.nodeCurve(cfg, 4)
}
func (c *YieldTermStructure) value(query int, t1, t2 float64, d Date, extrapolate bool) (float64, error) {
	var v C.double
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_curve_value(c.session.ctx, C.uint64_t(c.id), C.int32_t(query), C.double(t1), C.double(t2), C.int32_t(d.Serial()), C.bool(extrapolate), &v, &e), &e)
	})
	return float64(v), err
}
func (c *YieldTermStructure) Discount(t float64, extrapolate bool) (float64, error) {
	return c.value(0, t, 0, Date{}, extrapolate)
}
func (c *YieldTermStructure) DiscountDate(d Date, extrapolate bool) (float64, error) {
	return c.value(1, 0, 0, d, extrapolate)
}
func (c *YieldTermStructure) ZeroRate(t float64, extrapolate bool) (float64, error) {
	return c.value(2, t, 0, Date{}, extrapolate)
}
func (c *YieldTermStructure) ForwardRate(t1, t2 float64, extrapolate bool) (float64, error) {
	return c.value(3, t1, t2, Date{}, extrapolate)
}
func (c *YieldTermStructure) info(query int) (int32, error) {
	var v C.int32_t
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_curve_info(c.session.ctx, C.uint64_t(c.id), C.int32_t(query), &v, &e), &e)
	})
	return int32(v), err
}
func (c *YieldTermStructure) ReferenceDate() (Date, error) {
	v, e := c.info(0)
	return Date{serial: v}, e
}
func (c *YieldTermStructure) MaxDate() (Date, error)             { v, e := c.info(1); return Date{serial: v}, e }
func (c *YieldTermStructure) AllowsExtrapolation() (bool, error) { v, e := c.info(2); return v != 0, e }
func (c *YieldTermStructure) setExtrapolation(enabled bool) error {
	return c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_curve_extrapolation(c.session.ctx, C.uint64_t(c.id), C.bool(enabled), &e), &e)
	})
}
func (c *YieldTermStructure) EnableExtrapolation() error  { return c.setExtrapolation(true) }
func (c *YieldTermStructure) DisableExtrapolation() error { return c.setExtrapolation(false) }

type PiecewiseCurveConfig struct {
	AdditionalVariables *SimpleQuoteVariables
	AdditionalDates     func() ([]Date, error)
	AdditionalPenalties func(BootstrapState) ([]float64, error)
	ReferenceDate       Date
	Helpers             []*RateHelper
	DayCounter          *DayCounter
	Interpolation       string        // PiecewiseYieldCurve: LogLinear (default), Linear, Cubic.
	Bootstrap           string        // iterative (default), global; convex-monotone also supports local.
	AdditionalHelpers   []*RateHelper // global bootstrap only; extends maximum date.
}

func (s *Session) piecewise(cfg PiecewiseCurveConfig, kind int) (*YieldTermStructure, error) {
	if cfg.DayCounter == nil {
		return nil, errNilArgument("day counter")
	}
	if err := sameSession(s, cfg.DayCounter.object); err != nil {
		return nil, err
	}
	ids := make([]C.uint64_t, len(cfg.Helpers))
	extra := make([]C.uint64_t, len(cfg.AdditionalHelpers))
	for j, list := range [][]*RateHelper{cfg.Helpers, cfg.AdditionalHelpers} {
		for i, h := range list {
			if h == nil {
				return nil, errNilArgument("helper")
			}
			if err := sameSession(s, h.object); err != nil {
				return nil, err
			}
			if j == 0 {
				ids[i] = C.uint64_t(h.id)
			} else {
				extra[i] = C.uint64_t(h.id)
			}
		}
	}
	algo := 0
	switch cfg.Bootstrap {
	case "", "iterative":
	case "global":
		algo = 1
	case "local":
		algo = 2
	default:
		return nil, fmt.Errorf("unknown bootstrap %q", cfg.Bootstrap)
	}
	globalOptions := cfg.AdditionalVariables != nil || cfg.AdditionalDates != nil || cfg.AdditionalPenalties != nil
	if globalOptions && algo != 1 {
		return nil, fmt.Errorf("additional variables, dates and penalties require global bootstrap")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if globalOptions {
			return s.globalCurve(cfg, kind, ids, extra, &id)
		}
		var e C.ItofinError
		return ffiError(C.itofin_piecewise_curve_new(s.ctx, C.int32_t(cfg.ReferenceDate.Serial()), unsafe.SliceData(ids), C.size_t(len(ids)), C.uint64_t(cfg.DayCounter.id), C.int32_t(kind), C.int32_t(algo), unsafe.SliceData(extra), C.size_t(len(extra)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YieldTermStructure{object{s, uint64(id)}}, nil
}
func (s *Session) NewPiecewiseYieldCurve(cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
	kind := 0
	switch cfg.Interpolation {
	case "", "LogLinear":
	case "Linear":
		kind = 1
	case "Cubic":
		kind = 2
	default:
		return nil, fmt.Errorf("unknown interpolation %q", cfg.Interpolation)
	}
	return s.piecewise(cfg, kind)
}
func (s *Session) NewPiecewiseLogLinearDiscount(cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
	return s.piecewise(cfg, 0)
}
func (s *Session) NewPiecewiseLinearZero(cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
	return s.piecewise(cfg, 3)
}
func (s *Session) NewPiecewiseCubicZero(cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
	return s.piecewise(cfg, 4)
}
func (s *Session) NewPiecewiseLinearForward(cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
	return s.piecewise(cfg, 5)
}
func (s *Session) NewPiecewiseConvexMonotoneForward(cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
	return s.piecewise(cfg, 6)
}
func (s *Session) NewPiecewiseFlatForward(cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
	return s.piecewise(cfg, 7)
}
func (c *YieldTermStructure) Nodes() ([]Date, []float64, error) {
	var ds []C.int32_t
	var vs []float64
	err := c.session.invoke(func() error {
		var e C.ItofinError
		var n C.size_t
		if err := ffiError(C.itofin_curve_nodes(c.session.ctx, C.uint64_t(c.id), nil, nil, 0, &n, &e), &e); err != nil {
			return err
		}
		ds = make([]C.int32_t, int(n))
		vs = make([]float64, int(n))
		return ffiError(C.itofin_curve_nodes(c.session.ctx, C.uint64_t(c.id), unsafe.SliceData(ds), (*C.double)(unsafe.Pointer(unsafe.SliceData(vs))), n, &n, &e), &e)
	})
	if err != nil {
		return nil, nil, err
	}
	dates := make([]Date, len(ds))
	for i, d := range ds {
		dates[i] = Date{serial: int32(d)}
	}
	return dates, vs, nil
}
func (c *YieldTermStructure) Dates() ([]Date, error)   { ds, _, e := c.Nodes(); return ds, e }
func (c *YieldTermStructure) Data() ([]float64, error) { _, vs, e := c.Nodes(); return vs, e }
