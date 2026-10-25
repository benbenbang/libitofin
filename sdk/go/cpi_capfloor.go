package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type CpiCapFloorTermPriceSurface struct{ object }
type InterpolatingCpiCapFloorEngine struct{ object }
type CpiCapFloor struct{ object }

// CpiPriceSurfaceConfig creates an immutable bilinear quoted-price snapshot.
// MaturityYears must be positive and increasing. Prices use strike rows and
// maturity columns. Rebuild after curve, fixing or evaluation-date changes.
// Nominal is metadata only: neither parity nor the engine rescales quotes.
type CpiPriceSurfaceConfig struct {
	Nominal, BaseRate        float64
	ObservationLag           Period
	Calendar                 *Calendar
	Convention               BusinessDayConvention
	DayCounter               *DayCounter
	Index                    *ZeroInflationIndex
	Interpolation            CpiInterpolationType
	Discount                 *YieldTermStructure
	CapStrikes, FloorStrikes []float64
	MaturityYears            []int32
	CapPrices, FloorPrices   [][]float64
	Settings                 *Settings
}

func (s *Session) NewCpiCapFloorTermPriceSurface(a CpiPriceSurfaceConfig) (*CpiCapFloorTermPriceSurface, error) {
	if a.Calendar == nil || a.DayCounter == nil || a.Index == nil || a.Discount == nil || a.Settings == nil {
		return nil, fmt.Errorf("calendar, day counter, index, discount and settings required")
	}
	flatten := func(grid [][]float64, n int) ([]float64, error) {
		if len(grid) != n {
			return nil, fmt.Errorf("price matrix row count mismatch")
		}
		var out []float64
		for _, row := range grid {
			if len(row) != len(a.MaturityYears) {
				return nil, fmt.Errorf("price matrix column count mismatch")
			}
			out = append(out, row...)
		}
		return out, nil
	}
	caps, err := flatten(a.CapPrices, len(a.CapStrikes))
	if err != nil {
		return nil, err
	}
	floors, err := flatten(a.FloorPrices, len(a.FloorStrikes))
	if err != nil {
		return nil, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		if e := sameSession(s, a.Calendar.object, a.DayCounter.object, a.Index.object, a.Discount.object, a.Settings.object); e != nil {
			return e
		}
		cfg := C.ItofinCpiPriceSurfaceConfig{nominal: C.double(a.Nominal), base_rate: C.double(a.BaseRate), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), calendar: C.uint64_t(a.Calendar.id), convention: C.int32_t(a.Convention), day_counter: C.uint64_t(a.DayCounter.id), index: C.uint64_t(a.Index.id), interpolation: C.int32_t(a.Interpolation), discount: C.uint64_t(a.Discount.id), cap_strikes: (*C.double)(unsafe.Pointer(unsafe.SliceData(a.CapStrikes))), cap_strike_count: C.size_t(len(a.CapStrikes)), floor_strikes: (*C.double)(unsafe.Pointer(unsafe.SliceData(a.FloorStrikes))), floor_strike_count: C.size_t(len(a.FloorStrikes)), maturity_years: (*C.int32_t)(unsafe.Pointer(unsafe.SliceData(a.MaturityYears))), maturity_count: C.size_t(len(a.MaturityYears)), cap_prices: (*C.double)(unsafe.Pointer(unsafe.SliceData(caps))), cap_price_count: C.size_t(len(caps)), floor_prices: (*C.double)(unsafe.Pointer(unsafe.SliceData(floors))), floor_price_count: C.size_t(len(floors)), settings: C.uint64_t(a.Settings.id)}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_price_surface_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CpiCapFloorTermPriceSurface{object{s, uint64(id)}}, nil
}
func (v *CpiCapFloorTermPriceSurface) price(kind OptionType, date Date, strike float64) (float64, error) {
	if v == nil {
		return 0, errNilArgument("surface")
	}
	var out C.double
	err := v.session.invoke(func() error {
		if e := sameSession(v.session, v.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_price_surface_price(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.int32_t(date.Serial()), C.double(strike), &out, &e), &e)
	})
	return float64(out), err
}

// CapPrice returns a quoted cap price, with unrestricted bilinear extrapolation.
func (v *CpiCapFloorTermPriceSurface) CapPrice(date Date, strike float64) (float64, error) {
	return v.price(Call, date, strike)
}

// FloorPrice returns a quoted floor price, which can be negative on extrapolation.
func (v *CpiCapFloorTermPriceSurface) FloorPrice(date Date, strike float64) (float64, error) {
	return v.price(Put, date, strike)
}
func (s *Session) NewInterpolatingCpiCapFloorEngine(surface *CpiCapFloorTermPriceSurface) (*InterpolatingCpiCapFloorEngine, error) {
	if surface == nil {
		return nil, errNilArgument("surface")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, surface.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_capfloor_engine_new(s.ctx, C.uint64_t(surface.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &InterpolatingCpiCapFloorEngine{object{s, uint64(id)}}, nil
}

// CpiCapFloorConfig describes a single cumulative-inflation option.
// NPV is in surface-quote units, independent of Nominal, BaseCPI and Start.
// The contract expires after raw Maturity, not the adjusted payment date.
type CpiCapFloorConfig struct {
	Type                                OptionType
	Nominal, BaseCPI                    float64
	Start, Maturity                     Date
	FixingCalendar, PaymentCalendar     *Calendar
	FixingConvention, PaymentConvention BusinessDayConvention
	Strike                              float64
	Index                               *ZeroInflationIndex
	ObservationLag                      Period
	Interpolation                       CpiInterpolationType
	Settings                            *Settings
	Engine                              *InterpolatingCpiCapFloorEngine
}

func (s *Session) NewCpiCapFloor(a CpiCapFloorConfig) (*CpiCapFloor, error) {
	if a.FixingCalendar == nil || a.PaymentCalendar == nil || a.Index == nil || a.Settings == nil || a.Engine == nil {
		return nil, fmt.Errorf("calendars, index, settings and engine required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if e := sameSession(s, a.FixingCalendar.object, a.PaymentCalendar.object, a.Index.object, a.Settings.object, a.Engine.object); e != nil {
			return e
		}
		cfg := C.ItofinCpiCapFloorConfig{kind: C.int32_t(a.Type), nominal: C.double(a.Nominal), start: C.int32_t(a.Start.Serial()), base_cpi: C.double(a.BaseCPI), maturity: C.int32_t(a.Maturity.Serial()), fixing_calendar: C.uint64_t(a.FixingCalendar.id), fixing_convention: C.int32_t(a.FixingConvention), payment_calendar: C.uint64_t(a.PaymentCalendar.id), payment_convention: C.int32_t(a.PaymentConvention), strike: C.double(a.Strike), index: C.uint64_t(a.Index.id), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), settings: C.uint64_t(a.Settings.id), engine: C.uint64_t(a.Engine.id)}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_capfloor_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CpiCapFloor{object{s, uint64(id)}}, nil
}
func (v *CpiCapFloor) NPV() (float64, error) {
	if v == nil {
		return 0, errNilArgument("CPI cap/floor")
	}
	var out C.double
	err := v.session.invoke(func() error {
		if e := sameSession(v.session, v.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_capfloor_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *CpiCapFloor) Dates() (fixing Date, payment Date, err error) {
	if v == nil {
		return Date{}, Date{}, errNilArgument("CPI cap/floor")
	}
	var f, p C.int32_t
	err = v.session.invoke(func() error {
		if e := sameSession(v.session, v.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_capfloor_dates(v.session.ctx, C.uint64_t(v.id), &f, &p, &e), &e)
	})
	if err != nil {
		return Date{}, Date{}, err
	}
	return Date{serial: int32(f)}, Date{serial: int32(p)}, nil
}

// ReferenceDate returns the captured surface date, unaffected by settings rolls.
func (v *CpiCapFloorTermPriceSurface) ReferenceDate() (Date, error) {
	if v == nil {
		return Date{}, errNilArgument("surface")
	}
	var out C.int32_t
	err := v.session.invoke(func() error {
		if e := sameSession(v.session, v.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_price_surface_reference_date(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return Date{serial: int32(out)}, nil
}

// IsExpired compares the live evaluation date with raw, unadjusted maturity.
func (v *CpiCapFloor) IsExpired() (bool, error) {
	if v == nil {
		return false, errNilArgument("CPI cap/floor")
	}
	var out C.int32_t
	err := v.session.invoke(func() error {
		if e := sameSession(v.session, v.object); e != nil {
			return e
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_capfloor_is_expired(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return out != 0, err
}
