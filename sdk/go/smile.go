package itofin

/*
#include "itofin.h"
*/
import "C"

type VolatilityType int32

const (
	ShiftedLognormal VolatilityType = iota
	Normal
)

type SabrSmileSection struct{ object }
type SabrSmileConfig struct {
	ExerciseTime, Forward, Alpha, Beta, Nu, Rho, Shift float64
	VolatilityType                                     VolatilityType
}

func (s *Session) SabrSmileSection(cfg SabrSmileConfig) (*SabrSmileSection, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_sabr_smile_new(s.ctx, C.double(cfg.ExerciseTime), C.double(cfg.Forward), C.double(cfg.Alpha), C.double(cfg.Beta), C.double(cfg.Nu), C.double(cfg.Rho), C.double(cfg.Shift), C.int32_t(cfg.VolatilityType), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SabrSmileSection{object{s, uint64(id)}}, nil
}
func (v *SabrSmileSection) query(kind int32, strike float64) (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_sabr_smile_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.double(strike), &out, &e), &e)
	})
	return float64(out), err
}
func (v *SabrSmileSection) Volatility(strike float64) (float64, error) { return v.query(0, strike) }
func (v *SabrSmileSection) Variance(strike float64) (float64, error)   { return v.query(1, strike) }
func (v *SabrSmileSection) ExerciseTime() (float64, error)             { return v.query(2, 0) }
func (v *SabrSmileSection) ATMLevel() (float64, error)                 { return v.query(3, 0) }
func (v *SabrSmileSection) Alpha() (float64, error)                    { return v.query(4, 0) }
func (v *SabrSmileSection) Beta() (float64, error)                     { return v.query(5, 0) }
func (v *SabrSmileSection) Nu() (float64, error)                       { return v.query(6, 0) }
func (v *SabrSmileSection) Rho() (float64, error)                      { return v.query(7, 0) }
