package itofin

// #include "itofin.h"
import "C"
import "unsafe"

type MultiplicativePriceSeasonality struct{ object }

func (s *Session) NewMultiplicativePriceSeasonality(base Date, frequency Frequency, factors []float64) (*MultiplicativePriceSeasonality, error) {
	var ptr *C.double
	if len(factors) > 0 {
		ptr = (*C.double)(unsafe.Pointer(&factors[0]))
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_seasonality_new(s.ctx, C.int32_t(base.Serial()), C.int32_t(frequency), ptr, C.size_t(len(factors)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &MultiplicativePriceSeasonality{object{s, uint64(id)}}, nil
}
func (p *MultiplicativePriceSeasonality) value(query int32, date Date) (float64, error) {
	if p == nil {
		return 0, errNilArgument("seasonality")
	}
	var v C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_seasonality_value(p.session.ctx, C.uint64_t(p.id), C.int32_t(query), C.int32_t(date.Serial()), &v, &e), &e)
	})
	return float64(v), err
}
func (p *MultiplicativePriceSeasonality) SeasonalityBaseDate() (Date, error) {
	v, e := p.value(0, Date{})
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(int32(v))
}
func (p *MultiplicativePriceSeasonality) Frequency() (Frequency, error) {
	v, e := p.value(1, Date{})
	return Frequency(v), e
}
func (p *MultiplicativePriceSeasonality) SeasonalityFactor(date Date) (float64, error) {
	return p.value(2, date)
}
func (p *MultiplicativePriceSeasonality) SeasonalityFactors() ([]float64, error) {
	if p == nil {
		return nil, errNilArgument("seasonality")
	}
	var values []float64
	err := p.session.invoke(func() error {
		var e C.ItofinError
		var n C.size_t
		if err := ffiError(C.itofin_seasonality_factors(p.session.ctx, C.uint64_t(p.id), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		if n == 0 {
			return nil
		}
		values = make([]float64, int(n))
		return ffiError(C.itofin_seasonality_factors(p.session.ctx, C.uint64_t(p.id), (*C.double)(unsafe.Pointer(&values[0])), n, &n, &e), &e)
	})
	return values, err
}

// KerkhofSeasonality accumulates twelve monthly factors for zero inflation rates.
// It shares inspectors and curve setters with MultiplicativePriceSeasonality.
// Year-on-year date queries return an error when this correction is installed.
type KerkhofSeasonality = MultiplicativePriceSeasonality

// NewKerkhofSeasonality copies twelve finite factors in calendar-month order.
// Element zero is unused; January to February uses element one. Replacing or
// clearing the correction through SetSeasonality notifies the curve's consumers.
func (s *Session) NewKerkhofSeasonality(base Date, factors []float64) (*KerkhofSeasonality, error) {
	var ptr *C.double
	if len(factors) > 0 {
		ptr = (*C.double)(unsafe.Pointer(&factors[0]))
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_kerkhof_seasonality_new(s.ctx, C.int32_t(base.Serial()), ptr, C.size_t(len(factors)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &KerkhofSeasonality{object{s, uint64(id)}}, nil
}
