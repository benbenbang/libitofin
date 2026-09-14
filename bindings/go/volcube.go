package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"errors"
	"fmt"
	"runtime"
)

// SwaptionVolatilityCube exposes its base volatility surface and concrete ATM query.
type SwaptionVolatilityCube struct {
	*SwaptionVolatilityStructure
	cube object
}
type SwaptionVolatilityCubeConfig struct {
	ATMVol                            *SwaptionVolatilityStructure
	OptionTenors, SwapTenors          []Period
	StrikeSpreads                     []float64
	VolSpreads                        [][]*SimpleQuote
	SwapIndexBase, ShortSwapIndexBase *SwapIndex
	Settings                          *Settings
	VegaWeightedSmileFit              bool
	// SABR-only configuration. Rows contain alpha, beta, nu, rho quotes.
	ParametersGuess              [][]*SimpleQuote
	IsParameterFixed             [4]bool
	IsATMCalibrated, UseMaxError bool
	MaxGuesses                   uint
	CutoffStrike                 float64
}

func (s *Session) InterpolatedSwaptionVolatilityCube(x SwaptionVolatilityCubeConfig) (*SwaptionVolatilityCube, error) {
	return s.volCube(x, 0)
}
func (s *Session) SabrSwaptionVolatilityCube(x SwaptionVolatilityCubeConfig) (*SwaptionVolatilityCube, error) {
	return s.volCube(x, 1)
}
func (s *Session) volCube(x SwaptionVolatilityCubeConfig, kind int32) (*SwaptionVolatilityCube, error) {
	if x.ATMVol == nil || x.SwapIndexBase == nil || x.ShortSwapIndexBase == nil || x.Settings == nil {
		return nil, fmt.Errorf("ATM surface, swap indices, and settings are required")
	}
	no, ns, nk := len(x.OptionTenors), len(x.SwapTenors), len(x.StrikeSpreads)
	if no == 0 || ns == 0 || nk == 0 || no > int(^uint(0)>>1)/ns {
		return nil, fmt.Errorf("invalid cube dimensions")
	}
	nodes := no * ns
	deps := []object{x.ATMVol.object, x.SwapIndexBase.object, x.ShortSwapIndexBase.object, x.Settings.object}
	flatten := func(grid [][]*SimpleQuote, cols int) ([]C.uint64_t, error) {
		if len(grid) != nodes {
			return nil, fmt.Errorf("quote grid must have one row per tenor pair")
		}
		var result []C.uint64_t
		for _, r := range grid {
			if len(r) != cols {
				return nil, fmt.Errorf("quote grid column mismatch")
			}
			for _, q := range r {
				if q == nil {
					return nil, fmt.Errorf("quote cannot be nil")
				}
				deps = append(deps, q.object)
				result = append(result, C.uint64_t(q.id))
			}
		}
		return result, nil
	}
	vols, err := flatten(x.VolSpreads, nk)
	if err != nil {
		return nil, err
	}
	var guesses []C.uint64_t
	if kind == 1 {
		guesses, err = flatten(x.ParametersGuess, 4)
		if err != nil {
			return nil, err
		}
	}
	if err = sameSession(s, deps...); err != nil {
		return nil, err
	}
	ol, ou := make([]C.int32_t, no), make([]C.int32_t, no)
	sl, su := make([]C.int32_t, ns), make([]C.int32_t, ns)
	for i, p := range x.OptionTenors {
		ol[i] = C.int32_t(p.Length)
		ou[i] = C.int32_t(p.Unit)
	}
	for i, p := range x.SwapTenors {
		sl[i] = C.int32_t(p.Length)
		su[i] = C.int32_t(p.Unit)
	}
	strikes := make([]C.double, nk)
	for i, v := range x.StrikeSpreads {
		strikes[i] = C.double(v)
	}
	var fixed [4]C.int32_t
	for i, v := range x.IsParameterFixed {
		if v {
			fixed[i] = 1
		}
	}
	if x.MaxGuesses == 0 {
		x.MaxGuesses = 50
	}
	if x.CutoffStrike == 0 {
		x.CutoffStrike = 0.0001
	}
	var out C.ItofinVolCubeHandles
	err = s.invoke(func() error {
		var pin runtime.Pinner
		defer pin.Unpin()
		pin.Pin(&ol[0])
		pin.Pin(&ou[0])
		pin.Pin(&sl[0])
		pin.Pin(&su[0])
		pin.Pin(&strikes[0])
		pin.Pin(&vols[0])
		pin.Pin(&fixed[0])
		cfg := C.ItofinVolCubeConfig{atm: C.uint64_t(x.ATMVol.id), index: C.uint64_t(x.SwapIndexBase.id), short_index: C.uint64_t(x.ShortSwapIndexBase.id), settings: C.uint64_t(x.Settings.id), option_lengths: &ol[0], option_units: &ou[0], options: C.size_t(no), swap_lengths: &sl[0], swap_units: &su[0], swaps: C.size_t(ns), strike_spreads: &strikes[0], strikes: C.size_t(nk), vol_spreads: &vols[0], vol_count: C.size_t(len(vols)), fixed: &fixed[0], max_guesses: C.size_t(x.MaxGuesses), cutoff_strike: C.double(x.CutoffStrike)}
		if len(guesses) > 0 {
			pin.Pin(&guesses[0])
			cfg.guesses = &guesses[0]
			cfg.guess_count = C.size_t(len(guesses))
		}
		if x.IsATMCalibrated {
			cfg.atm_calibrated = 1
		}
		if x.VegaWeightedSmileFit {
			cfg.vega_weighted = 1
		}
		if x.UseMaxError {
			cfg.use_max_error = 1
		}
		var e C.ItofinError
		return ffiError(C.itofin_swaption_vol_cube_new(s.ctx, C.int32_t(kind), &cfg, &out, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SwaptionVolatilityCube{&SwaptionVolatilityStructure{object{s, uint64(out.surface)}}, object{s, uint64(out.cube)}}, nil
}
func (v *SwaptionVolatilityCube) ATMStrikeFromTenor(option, swap Period) (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_swaption_vol_cube_atm(v.session.ctx, C.uint64_t(v.cube.id), C.int32_t(option.Length), C.int32_t(option.Unit), C.int32_t(swap.Length), C.int32_t(swap.Unit), &out, &e), &e)
	})
	return float64(out), err
}
func (v *SwaptionVolatilityCube) Close() error {
	return errors.Join(v.SwaptionVolatilityStructure.Close(), v.cube.Close())
}
