package itofin

/*
#include "itofin.h"
#include <stdlib.h>
*/
import "C"
import (
	"unsafe"
)

type CpiInterpolationType int32

const (
	CpiFlat CpiInterpolationType = iota
	CpiLinear
)

type inflationIndex struct {
	object
	kind int32
}
type ZeroInflationIndex struct{ inflationIndex }
type YoYInflationIndex struct{ inflationIndex }

func (s *Session) zeroInflationIndex(kind int32, settings *Settings) (*ZeroInflationIndex, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, settings.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_zero_index_new(s.ctx, C.int32_t(kind), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ZeroInflationIndex{inflationIndex{object{s, uint64(id)}, 0}}, nil
}
func (s *Session) NewUKRPI(settings *Settings) (*ZeroInflationIndex, error) {
	return s.zeroInflationIndex(0, settings)
}
func (s *Session) NewUKHICP(settings *Settings) (*ZeroInflationIndex, error) {
	return s.zeroInflationIndex(1, settings)
}
func (s *Session) NewEUHICP(settings *Settings) (*ZeroInflationIndex, error) {
	return s.zeroInflationIndex(2, settings)
}

type YoYInflationIndexConfig struct {
	FamilyName, RegionName, RegionCode     string
	Revised                                bool
	Frequency                              Frequency
	AvailabilityLag                        Period
	CurrencyName, CurrencyCode             string
	CurrencyNumericCode                    int32
	CurrencySymbol, CurrencyFractionSymbol string
	CurrencyFractionsPerUnit               int32
	Settings                               *Settings
}

func (s *Session) NewYoYInflationIndex(a YoYInflationIndexConfig) (*YoYInflationIndex, error) {
	if a.Settings == nil {
		return nil, errNilArgument("settings")
	}
	names := []string{a.FamilyName, a.RegionName, a.RegionCode, a.CurrencyName, a.CurrencyCode, a.CurrencySymbol, a.CurrencyFractionSymbol}
	ptrs := make([]*C.char, len(names))
	for i, v := range names {
		ptrs[i] = C.CString(v)
		defer C.free(unsafe.Pointer(ptrs[i]))
	}
	cfg := C.ItofinYoyIndexConfig{family: ptrs[0], family_length: C.size_t(len(names[0])), region_name: ptrs[1], region_name_length: C.size_t(len(names[1])), region_code: ptrs[2], region_code_length: C.size_t(len(names[2])), revised: creditBool(a.Revised), frequency: C.int32_t(a.Frequency), lag_length: C.int32_t(a.AvailabilityLag.Length), lag_unit: C.int32_t(a.AvailabilityLag.Unit), currency_name: ptrs[3], currency_name_length: C.size_t(len(names[3])), currency_code: ptrs[4], currency_code_length: C.size_t(len(names[4])), currency_numeric: C.int32_t(a.CurrencyNumericCode), currency_symbol: ptrs[5], currency_symbol_length: C.size_t(len(names[5])), currency_fraction_symbol: ptrs[6], currency_fraction_symbol_length: C.size_t(len(names[6])), currency_fractions: C.int32_t(a.CurrencyFractionsPerUnit), settings: C.uint64_t(a.Settings.id)}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, a.Settings.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_yoy_index_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYInflationIndex{inflationIndex{object{s, uint64(id)}, 1}}, nil
}
func (s *Session) NewYoYInflationIndexFromUnderlying(zero *ZeroInflationIndex) (*YoYInflationIndex, error) {
	if zero == nil {
		return nil, errNilArgument("underlying")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, zero.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_yoy_index_from_underlying(s.ctx, C.uint64_t(zero.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYInflationIndex{inflationIndex{object{s, uint64(id)}, 1}}, nil
}
func (i *YoYInflationIndex) UnderlyingIndex() (*ZeroInflationIndex, error) {
	if i == nil {
		return nil, errNilArgument("index")
	}
	var id C.uint64_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_index_underlying(i.session.ctx, C.uint64_t(i.id), &id, &e), &e)
	})
	if err != nil || id == 0 {
		return nil, err
	}
	return &ZeroInflationIndex{inflationIndex{object{i.session, uint64(id)}, 0}}, nil
}
func (i inflationIndex) Name() (string, error) {
	var name string
	err := i.session.invoke(func() error {
		var e C.ItofinError
		var n C.size_t
		if err := ffiError(C.itofin_inflation_index_name(i.session.ctx, C.uint64_t(i.id), C.int32_t(i.kind), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		buf := make([]C.char, int(n))
		if err := ffiError(C.itofin_inflation_index_name(i.session.ctx, C.uint64_t(i.id), C.int32_t(i.kind), &buf[0], n, &n, &e), &e); err != nil {
			return err
		}
		name = string(unsafe.Slice((*byte)(unsafe.Pointer(&buf[0])), int(n)-1))
		return nil
	})
	return name, err
}
func (i inflationIndex) String() string {
	v, e := i.Name()
	if e != nil {
		return "InflationIndex(error: " + e.Error() + ")"
	}
	if i.kind == 0 {
		return "ZeroInflationIndex(" + v + ")"
	}
	return "YoYInflationIndex(" + v + ")"
}
func (i inflationIndex) AddFixing(date Date, value float64) error {
	return i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_inflation_index_add_fixing(i.session.ctx, C.uint64_t(i.id), C.int32_t(i.kind), C.int32_t(date.Serial()), C.double(value), &e), &e)
	})
}
func (i inflationIndex) Fixing(date Date, forecastTodaysFixing bool) (float64, error) {
	var v C.double
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_inflation_index_fixing(i.session.ctx, C.uint64_t(i.id), C.int32_t(i.kind), C.int32_t(date.Serial()), creditBool(forecastTodaysFixing), &v, &e), &e)
	})
	return float64(v), err
}
func (i inflationIndex) info(query int32, date Date) (int32, error) {
	var v C.int32_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_inflation_index_info(i.session.ctx, C.uint64_t(i.id), C.int32_t(i.kind), C.int32_t(query), C.int32_t(date.Serial()), &v, &e), &e)
	})
	return int32(v), err
}
func (i inflationIndex) NeedsForecast(date Date) (bool, error) {
	v, e := i.info(0, date)
	return v != 0, e
}
func (i inflationIndex) LastFixingDate() (Date, error) {
	v, e := i.info(1, Date{})
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(v)
}
func (i *YoYInflationIndex) Ratio() (bool, error) {
	if i == nil {
		return false, errNilArgument("index")
	}
	v, e := i.info(2, Date{})
	return v != 0, e
}
func (i inflationIndex) link(curve object) error {
	return i.session.invoke(func() error {
		if e := sameSession(i.session, i.object, curve); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_inflation_index_link(i.session.ctx, C.uint64_t(i.id), C.int32_t(i.kind), C.uint64_t(curve.id), &e), &e)
	})
}
func (i *ZeroInflationIndex) LinkTo(curve *ZeroInflationTermStructure) error {
	if i == nil || curve == nil {
		return errNilArgument("index or curve")
	}
	return i.link(curve.object)
}
func (i *YoYInflationIndex) LinkTo(curve *YoYInflationTermStructure) error {
	if i == nil || curve == nil {
		return errNilArgument("index or curve")
	}
	return i.link(curve.object)
}
