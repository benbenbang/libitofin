package itofin

/*
#include "itofin.h"
*/
import "C"

type GJRParameters struct {
	DailyVariance, Omega, Alpha, Beta, Gamma, Lambda, DaysPerYear float64
}

type GJRScheme int32

const (
	GJRPartialTruncation GJRScheme = iota
	GJRFullTruncation
	GJRReflection
)

type GJRProcess struct{ object }

func (s *Session) NewGJRProcess(spot *SimpleQuote, riskFree, dividend *YieldTermStructure, params GJRParameters, scheme GJRScheme) (*GJRProcess, error) {
	if spot == nil || riskFree == nil || dividend == nil {
		return nil, errNilArgument("market input")
	}
	if err := sameSession(s, spot.object, riskFree.object, dividend.object); err != nil {
		return nil, err
	}
	input := C.ItofinGjrParameters{
		daily_variance: C.double(params.DailyVariance), omega: C.double(params.Omega),
		alpha: C.double(params.Alpha), beta: C.double(params.Beta), gamma: C.double(params.Gamma),
		lambda: C.double(params.Lambda), days_per_year: C.double(params.DaysPerYear),
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_process_new(s.ctx, C.uint64_t(spot.id), C.uint64_t(riskFree.id), C.uint64_t(dividend.id), &input, C.int32_t(scheme), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GJRProcess{object{s, uint64(id)}}, nil
}

func (p *GJRProcess) query(kind int32, t float64, state []float64, out []float64) error {
	if p == nil {
		return errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return err
	}
	return p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_process_query(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), C.double(t), doubles(state), C.size_t(len(state)), doubles(out), C.size_t(len(out)), &e), &e)
	})
}

func (p *GJRProcess) InitialValues() ([2]float64, error) {
	var out [2]float64
	err := p.query(0, 0, nil, out[:])
	return out, err
}

func (p *GJRProcess) Parameters() (GJRParameters, error) {
	var out [7]float64
	if err := p.query(3, 0, nil, out[:]); err != nil {
		return GJRParameters{}, err
	}
	return GJRParameters{out[0], out[1], out[2], out[3], out[4], out[5], out[6]}, nil
}

func (p *GJRProcess) Discretization() (GJRScheme, error) {
	var out [1]float64
	err := p.query(4, 0, nil, out[:])
	return GJRScheme(out[0]), err
}

func (p *GJRProcess) Drift(t float64, state [2]float64) ([2]float64, error) {
	var out [2]float64
	err := p.query(1, t, state[:], out[:])
	return out, err
}

func (p *GJRProcess) Diffusion(t float64, state [2]float64) ([2][2]float64, error) {
	var out [4]float64
	err := p.query(2, t, state[:], out[:])
	return [2][2]float64{{out[0], out[1]}, {out[2], out[3]}}, err
}

func (p *GJRProcess) Evolve(t0 float64, state [2]float64, dt float64, draws [2]float64) ([2]float64, error) {
	var out [2]float64
	if p == nil {
		return out, errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return out, err
	}
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_process_evolve(p.session.ctx, C.uint64_t(p.id), C.double(t0), doubles(state[:]), C.size_t(len(state)), C.double(dt), doubles(draws[:]), C.size_t(len(draws)), doubles(out[:]), C.size_t(len(out)), &e), &e)
	})
	return out, err
}

func (p *GJRProcess) Time(date Date) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return 0, err
	}
	var out C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_process_time(p.session.ctx, C.uint64_t(p.id), C.int32_t(date.Serial()), &out, &e), &e)
	})
	return float64(out), err
}
