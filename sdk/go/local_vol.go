package itofin

/*
#include "itofin.h"
*/
import "C"

// LocalVolTermStructure owns a local volatility surface or curve. Only the
// linear variance curve is bound (#624); FD local-vol pricing is #828.
type LocalVolTermStructure struct{ object }

// NewLocalVolSurface derives the Dupire local volatility of a Black surface;
// the surface observes the spot quote.
func (s *Session) NewLocalVolSurface(black *BlackVolTermStructure, riskFree, dividend *YieldTermStructure, spot *SimpleQuote) (*LocalVolTermStructure, error) {
	if black == nil || riskFree == nil || dividend == nil || spot == nil {
		return nil, errNilArgument("market input")
	}
	if err := sameSession(s, black.object, riskFree.object, dividend.object, spot.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_local_vol_surface_new(s.ctx, C.uint64_t(black.id), C.uint64_t(riskFree.id), C.uint64_t(dividend.id), C.uint64_t(spot.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &LocalVolTermStructure{object{s, uint64(id)}}, nil
}

// LocalVol returns the local volatility at time t and underlying level strike.
func (v *LocalVolTermStructure) LocalVol(t, strike float64, extrapolate bool) (float64, error) {
	var out C.double
	var flag C.int32_t
	if extrapolate {
		flag = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_local_vol_query(v.session.ctx, C.uint64_t(v.id), C.double(t), C.double(strike), flag, &out, &e), &e)
	})
	return float64(out), err
}

// NewBlackScholesProcessWithLocalVol links spot and replaces the local
// volatility derived from vol with the supplied one.
func (s *Session) NewBlackScholesProcessWithLocalVol(spot *SimpleQuote, riskFree, dividend *YieldTermStructure, vol *BlackVolTermStructure, localVol *LocalVolTermStructure) (*BlackScholesProcess, error) {
	if spot == nil || riskFree == nil || dividend == nil || vol == nil || localVol == nil {
		return nil, errNilArgument("market input")
	}
	if err := sameSession(s, spot.object, riskFree.object, dividend.object, vol.object, localVol.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_scholes_with_local_vol(s.ctx, C.uint64_t(spot.id), C.uint64_t(riskFree.id), C.uint64_t(dividend.id), C.uint64_t(vol.id), C.uint64_t(localVol.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackScholesProcess{object{s, uint64(id)}}, nil
}

// NewLocalVolCurve builds the local volatility of the linear Black variance
// curve described by cfg, with the same validation as BlackVarianceCurve.
func (s *Session) NewLocalVolCurve(cfg BlackVarianceCurveConfig) (*LocalVolTermStructure, error) {
	dates, vols, monotone, err := s.varianceCurveArgs(cfg)
	if err != nil {
		return nil, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_local_vol_curve_new(s.ctx, C.int32_t(cfg.ReferenceDate.serial), &dates[0], &vols[0], C.size_t(len(dates)), C.uint64_t(cfg.DayCounter.id), monotone, C.int32_t(cfg.TimeExtrapolation), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &LocalVolTermStructure{object{s, uint64(id)}}, nil
}
