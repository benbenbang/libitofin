package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// BlackVolTermStructure owns a constant, curve, or surface Black volatility.
type BlackVolTermStructure struct{ object }
type BlackVolTimeExtrapolation int32

const (
	FlatVolatility BlackVolTimeExtrapolation = iota
	UseInterpolator
	LinearVariance
)

type BlackConstantVolConfig struct {
	ReferenceDate Date
	Volatility    float64
	DayCounter    *DayCounter
	Calendar      *Calendar
}

func (s *Session) BlackConstantVol(cfg BlackConstantVolConfig) (*BlackVolTermStructure, error) {
	if cfg.DayCounter == nil {
		return nil, fmt.Errorf("day counter is required")
	}
	deps := []object{cfg.DayCounter.object}
	var cal uint64
	if cfg.Calendar != nil {
		deps = append(deps, cfg.Calendar.object)
		cal = cfg.Calendar.id
	}
	if err := sameSession(s, deps...); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_constant_vol_new(s.ctx, C.int32_t(cfg.ReferenceDate.serial), C.double(cfg.Volatility), C.uint64_t(cfg.DayCounter.id), C.uint64_t(cal), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackVolTermStructure{object{s, uint64(id)}}, nil
}

type BlackVarianceCurveConfig struct {
	ReferenceDate         Date
	Dates                 []Date
	Volatilities          []float64
	DayCounter            *DayCounter
	ForceMonotoneVariance bool
	TimeExtrapolation     BlackVolTimeExtrapolation
}

func (s *Session) BlackVarianceCurve(cfg BlackVarianceCurveConfig) (*BlackVolTermStructure, error) {
	if cfg.DayCounter == nil {
		return nil, fmt.Errorf("day counter is required")
	}
	if len(cfg.Dates) == 0 || len(cfg.Dates) != len(cfg.Volatilities) {
		return nil, fmt.Errorf("dates and volatilities must have equal nonzero lengths")
	}
	if err := sameSession(s, cfg.DayCounter.object); err != nil {
		return nil, err
	}
	dates := make([]C.int32_t, len(cfg.Dates))
	vols := make([]C.double, len(cfg.Volatilities))
	for i := range dates {
		dates[i] = C.int32_t(cfg.Dates[i].serial)
		vols[i] = C.double(cfg.Volatilities[i])
	}
	var monotone C.int32_t
	if cfg.ForceMonotoneVariance {
		monotone = 1
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_variance_curve_new(s.ctx, C.int32_t(cfg.ReferenceDate.serial), &dates[0], &vols[0], C.size_t(len(dates)), C.uint64_t(cfg.DayCounter.id), monotone, C.int32_t(cfg.TimeExtrapolation), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackVolTermStructure{object{s, uint64(id)}}, nil
}

type BlackVarianceSurfaceConfig struct {
	ReferenceDate Date
	Dates         []Date
	Strikes       []float64
	// Volatilities contains one row per strike, one column per expiry.
	Volatilities [][]float64
	DayCounter   *DayCounter
	Calendar     *Calendar
}

func (s *Session) BlackVarianceSurface(cfg BlackVarianceSurfaceConfig) (*BlackVolTermStructure, error) {
	if cfg.DayCounter == nil {
		return nil, fmt.Errorf("day counter is required")
	}
	if len(cfg.Dates) == 0 || len(cfg.Strikes) == 0 || len(cfg.Volatilities) != len(cfg.Strikes) {
		return nil, fmt.Errorf("invalid volatility matrix dimensions")
	}
	deps := []object{cfg.DayCounter.object}
	var cal uint64
	if cfg.Calendar != nil {
		deps = append(deps, cfg.Calendar.object)
		cal = cfg.Calendar.id
	}
	if err := sameSession(s, deps...); err != nil {
		return nil, err
	}
	dates := make([]C.int32_t, len(cfg.Dates))
	strikes := make([]C.double, len(cfg.Strikes))
	var vols []C.double
	for i, d := range cfg.Dates {
		dates[i] = C.int32_t(d.serial)
	}
	for i, row := range cfg.Volatilities {
		if len(row) != len(dates) {
			return nil, fmt.Errorf("volatility matrix is ragged")
		}
		strikes[i] = C.double(cfg.Strikes[i])
		for _, v := range row {
			vols = append(vols, C.double(v))
		}
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_variance_surface_new(s.ctx, C.int32_t(cfg.ReferenceDate.serial), &dates[0], C.size_t(len(dates)), &strikes[0], C.size_t(len(strikes)), &vols[0], C.size_t(len(vols)), C.uint64_t(cfg.DayCounter.id), C.uint64_t(cal), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackVolTermStructure{object{s, uint64(id)}}, nil
}
func (v *BlackVolTermStructure) query(kind int32, t1, t2, strike float64, extrapolate bool) (float64, error) {
	var out C.double
	var ext C.int32_t
	if extrapolate {
		ext = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_vol_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.double(t1), C.double(t2), C.double(strike), ext, &out, &e), &e)
	})
	return float64(out), err
}
func (v *BlackVolTermStructure) BlackVol(t, strike float64, extrapolate bool) (float64, error) {
	return v.query(0, t, 0, strike, extrapolate)
}
func (v *BlackVolTermStructure) BlackVariance(t, strike float64, extrapolate bool) (float64, error) {
	return v.query(1, t, 0, strike, extrapolate)
}
func (v *BlackVolTermStructure) BlackForwardVol(t1, t2, strike float64, extrapolate bool) (float64, error) {
	return v.query(2, t1, t2, strike, extrapolate)
}
func (v *BlackVolTermStructure) BlackForwardVariance(t1, t2, strike float64, extrapolate bool) (float64, error) {
	return v.query(3, t1, t2, strike, extrapolate)
}
func (v *BlackVolTermStructure) MinStrike() (float64, error) { return v.query(4, 0, 0, 0, false) }
func (v *BlackVolTermStructure) MaxStrike() (float64, error) { return v.query(5, 0, 0, 0, false) }
func (v *BlackVolTermStructure) dateQuery(kind int32, date Date, strike float64, extrapolate bool) (float64, error) {
	var out C.double
	var ext C.int32_t
	if extrapolate {
		ext = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_vol_date_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.int32_t(date.serial), C.double(strike), ext, &out, &e), &e)
	})
	return float64(out), err
}
func (v *BlackVolTermStructure) BlackVolDate(d Date, strike float64, extrapolate bool) (float64, error) {
	return v.dateQuery(0, d, strike, extrapolate)
}
func (v *BlackVolTermStructure) BlackVarianceDate(d Date, strike float64, extrapolate bool) (float64, error) {
	return v.dateQuery(1, d, strike, extrapolate)
}
func (v *BlackVolTermStructure) control(action int32) (int32, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_vol_control(v.session.ctx, C.uint64_t(v.id), C.int32_t(action), &out, &e), &e)
	})
	return int32(out), err
}
func (v *BlackVolTermStructure) MaxDate() (Date, error) {
	serial, err := v.control(0)
	return Date{serial: serial}, err
}
func (v *BlackVolTermStructure) AllowsExtrapolation() (bool, error) {
	x, err := v.control(1)
	return x != 0, err
}
func (v *BlackVolTermStructure) EnableExtrapolation() error  { _, err := v.control(2); return err }
func (v *BlackVolTermStructure) DisableExtrapolation() error { _, err := v.control(3); return err }
