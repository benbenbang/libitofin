package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type YoYCapFloorTermPriceSurface struct{ object }
type KInterpolatedYoYOptionletVolatilitySurface struct{ object }
type YoYCapFloorTermPriceSurfaceConfig struct {
	FixingDays               uint32
	ObservationLag           Period
	Index                    *YoYInflationIndex
	Interpolation            CpiInterpolationType
	NominalTermStructure     *YieldTermStructure
	DayCounter               *DayCounter
	Calendar                 *Calendar
	Convention               BusinessDayConvention
	CapStrikes, FloorStrikes []float64
	Maturities               []Period
	CapPrices, FloorPrices   [][]float64
	Settings                 *Settings
}

func inflationMatrix(rows [][]float64, expectedRows, expectedColumns int) ([]float64, error) {
	if len(rows) != expectedRows || expectedRows == 0 || expectedColumns == 0 {
		return nil, fmt.Errorf("invalid price matrix shape")
	}
	var flat []float64
	for _, row := range rows {
		if len(row) != expectedColumns {
			return nil, fmt.Errorf("ragged price matrix")
		}
		flat = append(flat, row...)
	}
	return flat, nil
}
func (s *Session) NewYoYCapFloorTermPriceSurface(a YoYCapFloorTermPriceSurfaceConfig) (*YoYCapFloorTermPriceSurface, error) {
	if a.Index == nil || a.NominalTermStructure == nil || a.DayCounter == nil || a.Calendar == nil || a.Settings == nil {
		return nil, fmt.Errorf("index, curve, day counter, calendar and settings required")
	}
	if err := sameSession(s, a.Index.object, a.NominalTermStructure.object, a.DayCounter.object, a.Calendar.object, a.Settings.object); err != nil {
		return nil, err
	}
	caps, err := inflationMatrix(a.CapPrices, len(a.CapStrikes), len(a.Maturities))
	if err != nil {
		return nil, err
	}
	floors, err := inflationMatrix(a.FloorPrices, len(a.FloorStrikes), len(a.Maturities))
	if err != nil {
		return nil, err
	}
	periods := make([]C.ItofinInflationPeriod, len(a.Maturities))
	for i, p := range a.Maturities {
		periods[i] = C.ItofinInflationPeriod{length: C.int32_t(p.Length), unit: C.int32_t(p.Unit)}
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		var e C.ItofinError
		cfg := C.ItofinYoYPriceSurfaceConfig{fixing_days: C.uint32_t(a.FixingDays), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), index: C.uint64_t(a.Index.id), interpolation: C.int32_t(a.Interpolation), nominal: C.uint64_t(a.NominalTermStructure.id), day_counter: C.uint64_t(a.DayCounter.id), calendar: C.uint64_t(a.Calendar.id), convention: C.int32_t(a.Convention), cap_strikes: inflationDoubles(a.CapStrikes), cap_count: C.size_t(len(a.CapStrikes)), floor_strikes: inflationDoubles(a.FloorStrikes), floor_count: C.size_t(len(a.FloorStrikes)), maturities: &periods[0], maturity_count: C.size_t(len(periods)), cap_prices: inflationDoubles(caps), cap_price_count: C.size_t(len(caps)), floor_prices: inflationDoubles(floors), floor_price_count: C.size_t(len(floors)), settings: C.uint64_t(a.Settings.id)}
		return ffiError(C.itofin_yoy_price_surface_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYCapFloorTermPriceSurface{object{s, uint64(id)}}, nil
}
func (v *YoYCapFloorTermPriceSurface) value(query int32, d Date, strike float64, extrapolate bool) (float64, error) {
	var out C.double
	var flag C.uint8_t
	if extrapolate {
		flag = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_price_surface_value(v.session.ctx, C.uint64_t(v.id), C.int32_t(query), C.int32_t(d.serial), C.double(strike), flag, &out, &e), &e)
	})
	return float64(out), err
}
func (v *YoYCapFloorTermPriceSurface) CapPrice(d Date, strike float64) (float64, error) {
	return v.value(0, d, strike, true)
}
func (v *YoYCapFloorTermPriceSurface) FloorPrice(d Date, strike float64) (float64, error) {
	return v.value(1, d, strike, true)
}
func (v *YoYCapFloorTermPriceSurface) ATMYoYSwapRate(d Date, extrapolate bool) (float64, error) {
	return v.value(2, d, 0, extrapolate)
}
func (v *YoYCapFloorTermPriceSurface) Strikes() ([]float64, error) {
	var result []float64
	err := v.session.invoke(func() error {
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_price_surface_strikes(v.session.ctx, C.uint64_t(v.id), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		result = make([]float64, int(n))
		if n == 0 {
			return nil
		}
		return ffiError(C.itofin_yoy_price_surface_strikes(v.session.ctx, C.uint64_t(v.id), inflationDoubles(result), n, &n, &e), &e)
	})
	return result, err
}
func (v *YoYCapFloorTermPriceSurface) Maturities() ([]Period, error) {
	var result []Period
	err := v.session.invoke(func() error {
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_price_surface_maturities(v.session.ctx, C.uint64_t(v.id), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		result = make([]Period, int(n))
		if n == 0 {
			return nil
		}
		native := make([]C.ItofinInflationPeriod, int(n))
		if err := ffiError(C.itofin_yoy_price_surface_maturities(v.session.ctx, C.uint64_t(v.id), &native[0], n, &n, &e), &e); err != nil {
			return err
		}
		for i, p := range native {
			result[i] = Period{int32(p.length), TimeUnit(p.unit)}
		}
		return nil
	})
	return result, err
}

type KInterpolatedYoYOptionletVolatilitySurfaceConfig struct {
	SettlementDays       uint32
	Calendar             *Calendar
	Convention           BusinessDayConvention
	DayCounter           *DayCounter
	ObservationLag       Period
	CapFloorPrices       *YoYCapFloorTermPriceSurface
	Index                *YoYInflationIndex
	NominalTermStructure *YieldTermStructure
	Slope                float64
	Settings             *Settings
}

func (s *Session) NewKInterpolatedYoYOptionletVolatilitySurface(a KInterpolatedYoYOptionletVolatilitySurfaceConfig) (*KInterpolatedYoYOptionletVolatilitySurface, error) {
	if a.Calendar == nil || a.DayCounter == nil || a.CapFloorPrices == nil || a.Index == nil || a.NominalTermStructure == nil || a.Settings == nil {
		return nil, fmt.Errorf("calendar, day counter, prices, index, nominal curve and settings required")
	}
	if err := sameSession(s, a.Calendar.object, a.DayCounter.object, a.CapFloorPrices.object, a.Index.object, a.NominalTermStructure.object, a.Settings.object); err != nil {
		return nil, err
	}
	cfg := C.ItofinKYoYVolConfig{settlement_days: C.uint32_t(a.SettlementDays), calendar: C.uint64_t(a.Calendar.id), convention: C.int32_t(a.Convention), day_counter: C.uint64_t(a.DayCounter.id), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), prices: C.uint64_t(a.CapFloorPrices.id), index: C.uint64_t(a.Index.id), nominal: C.uint64_t(a.NominalTermStructure.id), slope: C.double(a.Slope), settings: C.uint64_t(a.Settings.id)}
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_k_yoy_vol_new(s.ctx, cfg, &id, &e), &e) })
	if err != nil {
		return nil, err
	}
	return &KInterpolatedYoYOptionletVolatilitySurface{object{s, uint64(id)}}, nil
}
func (v *KInterpolatedYoYOptionletVolatilitySurface) query(query int32, d Date, strike float64) (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_k_yoy_vol_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(query), C.int32_t(d.serial), C.double(strike), &out, &e), &e)
	})
	return float64(out), err
}
func (v *KInterpolatedYoYOptionletVolatilitySurface) Volatility(d Date, strike float64) (float64, error) {
	return v.query(0, d, strike)
}
func (v *KInterpolatedYoYOptionletVolatilitySurface) BaseDate() (Date, error) {
	n, err := v.query(1, Date{}, 0)
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(n))
}
func (v *KInterpolatedYoYOptionletVolatilitySurface) MinStrike() (float64, error) {
	return v.query(2, Date{}, 0)
}
func (v *KInterpolatedYoYOptionletVolatilitySurface) MaxStrike() (float64, error) {
	return v.query(3, Date{}, 0)
}
func (v *KInterpolatedYoYOptionletVolatilitySurface) MaxDate() (Date, error) {
	n, err := v.query(4, Date{}, 0)
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(n))
}
func (v *KInterpolatedYoYOptionletVolatilitySurface) DSlice(d Date) (strikes, volatilities []float64, err error) {
	err = v.session.invoke(func() error {
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_k_yoy_vol_slice(v.session.ctx, C.uint64_t(v.id), C.int32_t(d.serial), nil, nil, 0, &n, &e), &e); err != nil {
			return err
		}
		strikes = make([]float64, int(n))
		volatilities = make([]float64, int(n))
		if n == 0 {
			return nil
		}
		return ffiError(C.itofin_k_yoy_vol_slice(v.session.ctx, C.uint64_t(v.id), C.int32_t(d.serial), inflationDoubles(strikes), inflationDoubles(volatilities), n, &n, &e), &e)
	})
	return
}
