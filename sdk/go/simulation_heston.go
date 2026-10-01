package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
)

// HestonScheme selects one of the Heston process's supported variance schemes.
type HestonScheme int32

const (
	HestonQEM HestonScheme = 0
	HestonQE  HestonScheme = 1
)

// HestonConfig describes flat-rate Heston simulation. The seed must be nonzero.
type HestonConfig struct {
	Spot, Variance              float64
	RiskFreeRate, DividendYield float64
	Kappa, Theta, Sigma, Rho    float64
	Horizon                     float64
	Steps, Paths                int
	Seed                        uint32
	Scheme                      HestonScheme
	TerminalOnly                bool
	MaxOutputValues             int
}

// SimulateHeston returns path/time/component values; component zero is spot and
// one is variance. Terminal-only output equals the last full row for each path.
// Independent draws are consumed path, time, spot-factor, variance-factor.
func SimulateHeston(c HestonConfig) (*Simulation, error) {
	if c.Steps <= 0 || c.Paths <= 0 || c.Seed == 0 || (c.Scheme != HestonQE && c.Scheme != HestonQEM) {
		return nil, fmt.Errorf("itofin: invalid Heston dimensions, seed, or scheme")
	}
	if math.IsNaN(c.Horizon) || math.IsInf(c.Horizon, 0) || c.Horizon < 0 {
		return nil, fmt.Errorf("itofin: invalid horizon")
	}
	if c.Steps == int(^uint(0)>>1) {
		return nil, fmt.Errorf("itofin: step count overflow")
	}
	times := 1
	if !c.TerminalOnly {
		times = c.Steps + 1
	}
	count, err := checkedProduct(c.Paths, times)
	if err != nil {
		return nil, err
	}
	count, err = checkedProduct(count, 2)
	if err != nil {
		return nil, err
	}
	limit := c.MaxOutputValues
	if limit == 0 {
		limit = DefaultMaxOutputValues
	}
	if limit < 0 || count > limit || count > int(^uint(0)>>1)/8 {
		return nil, fmt.Errorf("itofin: result exceeds output limit; use terminal mode or smaller batches")
	}
	var terminal C.int32_t
	if c.TerminalOnly {
		terminal = 1
	}
	input := C.ItofinHestonInput{
		spot: C.double(c.Spot), variance: C.double(c.Variance),
		risk_free_rate: C.double(c.RiskFreeRate), dividend_yield: C.double(c.DividendYield),
		kappa: C.double(c.Kappa), theta: C.double(c.Theta), sigma: C.double(c.Sigma), rho: C.double(c.Rho),
		horizon: C.double(c.Horizon), steps: C.size_t(c.Steps), paths: C.size_t(c.Paths),
		seed: C.uint32_t(c.Seed), scheme: C.int32_t(c.Scheme), terminal_only: terminal,
	}
	out := make([]float64, count)
	var e C.ItofinError
	err = ffiError(C.itofin_heston_paths(&input, doubles(out), C.size_t(len(out)), &e), &e)
	if err != nil {
		return nil, err
	}
	return &Simulation{Values: out, Paths: c.Paths, Times: times, Assets: 2, TerminalOnly: c.TerminalOnly}, nil
}
