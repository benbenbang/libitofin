package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type Position int32

const (
	PositionLong Position = iota
	PositionShort
)

type ForwardRateAgreement struct{ object }
type FRAConfig struct {
	Index            *IborIndex
	ValueDate        Date
	MaturityDate     *Date
	Position         Position
	Strike, Notional float64
	Discount         *YieldTermStructure
}

func (s *Session) NewForwardRateAgreement(a FRAConfig) (*ForwardRateAgreement, error) {
	if a.Index == nil {
		return nil, fmt.Errorf("FRA index required")
	}
	if a.MaturityDate != nil && a.MaturityDate.Serial() == 0 {
		return nil, fmt.Errorf("invalid explicit maturity date")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{a.Index.object}
		if a.Discount != nil {
			objects = append(objects, a.Discount.object)
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		c := C.ItofinFraConfig{index: C.uint64_t(a.Index.id), value_date: C.int32_t(a.ValueDate.Serial()), position: C.int32_t(a.Position), strike: C.double(a.Strike), notional: C.double(a.Notional)}
		if a.MaturityDate != nil {
			c.maturity_date = C.int32_t(a.MaturityDate.Serial())
		}
		if a.Discount != nil {
			c.discount = C.uint64_t(a.Discount.id)
		}
		var e C.ItofinError
		return ffiError(C.itofin_fra_new(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ForwardRateAgreement{object{s, uint64(id)}}, nil
}
func (f *ForwardRateAgreement) value(field int32) (float64, error) {
	var out C.double
	s := f.session
	err := s.invoke(func() error {
		if err := sameSession(s, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_fra_value(s.ctx, C.uint64_t(f.id), C.int32_t(field), &out, &e), &e)
	})
	return float64(out), err
}
func (f *ForwardRateAgreement) NPV() (float64, error)         { return f.value(0) }
func (f *ForwardRateAgreement) ForwardRate() (float64, error) { return f.value(1) }
func (f *ForwardRateAgreement) Amount() (float64, error)      { return f.value(2) }
func (f *ForwardRateAgreement) date(field int32) (Date, error) {
	var out C.int32_t
	s := f.session
	err := s.invoke(func() error {
		if err := sameSession(s, f.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_fra_date(s.ctx, C.uint64_t(f.id), C.int32_t(field), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(out))
}
func (f *ForwardRateAgreement) ValueDate() (Date, error)    { return f.date(0) }
func (f *ForwardRateAgreement) MaturityDate() (Date, error) { return f.date(1) }
