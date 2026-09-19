package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"
import "runtime"

// RateVolGridConfig uses option-tenor rows and swap-tenor or strike columns.
// Quotes selects live market data; Settings selects a moving reference date.
type RateVolGridConfig struct {
	ReferenceDate            Date
	SettlementDays           uint32
	Calendar                 *Calendar
	Convention               BusinessDayConvention
	DayCounter               *DayCounter
	Settings                 *Settings
	OptionTenors, SwapTenors []Period
	Strikes                  []float64
	Volatilities, Shifts     [][]float64
	Quotes                   [][]*SimpleQuote
	VolatilityType           VolatilityType
	FlatExtrapolation        bool
}

// All C arrays are Go-owned scalar buffers borrowed only during invoke.
func (s *Session) volGrid(x RateVolGridConfig, swaption bool, optionDates []Date) (uint64, error) {
	if x.Calendar == nil || x.DayCounter == nil {
		return 0, fmt.Errorf("calendar and day counter are required")
	}
	rows, cols := len(x.OptionTenors), len(x.Strikes)
	if optionDates != nil {
		if !swaption || len(x.OptionTenors) != 0 || x.Settings != nil || x.Quotes != nil {
			return 0, fmt.Errorf("option dates require fixed numeric swaption data without option tenors")
		}
		rows = len(optionDates)
	}
	if swaption {
		cols = len(x.SwapTenors)
	}
	if rows == 0 || cols == 0 {
		return 0, fmt.Errorf("grid axes must be nonempty")
	}
	deps := []object{x.Calendar.object, x.DayCounter.object}
	var cfg C.ItofinVolGridConfig
	cfg.reference_date = C.int32_t(x.ReferenceDate.serial)
	cfg.settlement_days = C.uint32_t(x.SettlementDays)
	cfg.calendar = C.uint64_t(x.Calendar.id)
	cfg.convention = C.int32_t(x.Convention)
	cfg.day_counter = C.uint64_t(x.DayCounter.id)
	cfg.rows = C.size_t(rows)
	cfg.columns = C.size_t(cols)
	cfg.volatility_type = C.int32_t(x.VolatilityType)
	if x.FlatExtrapolation {
		cfg.flat_extrapolation = 1
	}
	if x.Settings != nil {
		deps = append(deps, x.Settings.object)
		cfg.settings = C.uint64_t(x.Settings.id)
	}
	ol, ou := make([]C.int32_t, rows), make([]C.int32_t, rows)
	for i, p := range x.OptionTenors {
		ol[i] = C.int32_t(p.Length)
		ou[i] = C.int32_t(p.Unit)
	}
	dateSerials := make([]C.int32_t, len(optionDates))
	for i, date := range optionDates {
		dateSerials[i] = C.int32_t(date.serial)
	}
	sl, su := make([]C.int32_t, cols), make([]C.int32_t, cols)
	strikes := make([]C.double, cols)
	if swaption {
		for i, p := range x.SwapTenors {
			sl[i] = C.int32_t(p.Length)
			su[i] = C.int32_t(p.Unit)
		}
	} else {
		for i, v := range x.Strikes {
			strikes[i] = C.double(v)
		}
	}
	flatten := func(grid [][]float64) ([]C.double, error) {
		if len(grid) != rows {
			return nil, fmt.Errorf("grid row count mismatch")
		}
		var result []C.double
		for _, r := range grid {
			if len(r) != cols {
				return nil, fmt.Errorf("grid column count mismatch")
			}
			for _, v := range r {
				result = append(result, C.double(v))
			}
		}
		return result, nil
	}
	var values, shifts []C.double
	var quotes []C.uint64_t
	var err error
	if x.Quotes != nil {
		if len(x.Quotes) != rows {
			return 0, fmt.Errorf("quote grid row count mismatch")
		}
		for _, r := range x.Quotes {
			if len(r) != cols {
				return 0, fmt.Errorf("quote grid column count mismatch")
			}
			for _, q := range r {
				if q == nil {
					return 0, fmt.Errorf("quote must not be nil")
				}
				deps = append(deps, q.object)
				quotes = append(quotes, C.uint64_t(q.id))
			}
		}
	} else {
		values, err = flatten(x.Volatilities)
		if err != nil {
			return 0, err
		}
	}
	if x.Shifts != nil {
		shifts, err = flatten(x.Shifts)
		if err != nil {
			return 0, err
		}
	}
	if err = sameSession(s, deps...); err != nil {
		return 0, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		// Pin pointed-to scalar buffers for cgo nested-pointer safety.
		var pin runtime.Pinner
		defer pin.Unpin()
		pin.Pin(&ol[0])
		pin.Pin(&ou[0])
		pin.Pin(&sl[0])
		pin.Pin(&su[0])
		pin.Pin(&strikes[0])
		if len(values) > 0 {
			pin.Pin(&values[0])
		}
		if len(quotes) > 0 {
			pin.Pin(&quotes[0])
		}
		if len(shifts) > 0 {
			pin.Pin(&shifts[0])
		}
		cfg.option_lengths = &ol[0]
		cfg.option_units = &ou[0]
		cfg.swap_lengths = &sl[0]
		cfg.swap_units = &su[0]
		cfg.strikes = &strikes[0]
		cfg.count = C.size_t(rows) * C.size_t(cols)
		if len(values) > 0 {
			cfg.values = &values[0]
		}
		if len(quotes) > 0 {
			cfg.quotes = &quotes[0]
		}
		if len(shifts) > 0 {
			cfg.shifts = &shifts[0]
			cfg.shift_count = C.size_t(len(shifts))
		}
		var e C.ItofinError
		if swaption {
			if optionDates != nil {
				return ffiError(C.itofin_swaption_vol_matrix_dates(s.ctx, &cfg, &dateSerials[0], C.size_t(len(dateSerials)), &id, &e), &e)
			}
			if x.Settings == nil && x.Quotes != nil {
				return ffiError(C.itofin_swaption_vol_matrix_fixed_quotes(s.ctx, &cfg, &id, &e), &e)
			}
			if x.Settings != nil && x.Quotes == nil {
				return ffiError(C.itofin_swaption_vol_matrix_moving_matrix(s.ctx, &cfg, &id, &e), &e)
			}
			return ffiError(C.itofin_swaption_vol_matrix_new(s.ctx, &cfg, &id, &e), &e)
		}
		return ffiError(C.itofin_capfloor_vol_surface_new(s.ctx, &cfg, &id, &e), &e)
	})
	return uint64(id), err
}

// SwaptionVolatilityMatrix selects moving dates with Settings and live data with Quotes independently.
// Numeric volatility and shift grids are copied.
func (s *Session) SwaptionVolatilityMatrix(cfg RateVolGridConfig) (*SwaptionVolatilityStructure, error) {
	id, err := s.volGrid(cfg, true, nil)
	if err != nil {
		return nil, err
	}
	return &SwaptionVolatilityStructure{object{s, id}}, nil
}

// SwaptionVolatilityMatrixDates copies numeric market data on fixed option dates.
// Config must omit OptionTenors, Settings and Quotes.
func (s *Session) SwaptionVolatilityMatrixDates(cfg RateVolGridConfig, optionDates []Date) (*SwaptionVolatilityStructure, error) {
	if len(optionDates) == 0 {
		return nil, fmt.Errorf("option dates must be nonempty")
	}
	id, err := s.volGrid(cfg, true, optionDates)
	if err != nil {
		return nil, err
	}
	return &SwaptionVolatilityStructure{object{s, id}}, nil
}

type CapFloorTermVolSurface struct{ object }

func (s *Session) CapFloorTermVolSurface(cfg RateVolGridConfig) (*CapFloorTermVolSurface, error) {
	id, err := s.volGrid(cfg, false, nil)
	if err != nil {
		return nil, err
	}
	return &CapFloorTermVolSurface{object{s, id}}, nil
}
func (v *CapFloorTermVolSurface) query(kind int32, tenor Period, date Date, time, strike float64, extrapolate bool) (float64, error) {
	var out C.double
	var ext C.int32_t
	if extrapolate {
		ext = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_capfloor_vol_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.int32_t(tenor.Length), C.int32_t(tenor.Unit), C.int32_t(date.serial), C.double(time), C.double(strike), ext, &out, &e), &e)
	})
	return float64(out), err
}
func (v *CapFloorTermVolSurface) Volatility(tenor Period, strike float64, extrapolate bool) (float64, error) {
	return v.query(0, tenor, Date{}, 0, strike, extrapolate)
}
func (v *CapFloorTermVolSurface) VolatilityDate(date Date, strike float64, extrapolate bool) (float64, error) {
	return v.query(1, Period{}, date, 0, strike, extrapolate)
}
func (v *CapFloorTermVolSurface) VolatilityTime(time, strike float64, extrapolate bool) (float64, error) {
	return v.query(2, Period{}, Date{}, time, strike, extrapolate)
}
