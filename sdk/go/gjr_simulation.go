package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
)

type GJRConfig struct {
	Spot, DailyVariance, RiskFreeRate, DividendYield float64
	Omega, Alpha, Beta, Gamma, Lambda, DaysPerYear   float64
	Horizon                                          float64
	Steps, Paths                                     int
	Seed                                             uint32
	Scheme                                           GJRScheme
	TerminalOnly                                     bool
	MaxOutputValues                                  int
}

func SimulateGJR(c GJRConfig) (*Simulation, error) {
	if c.Steps <= 0 || c.Paths <= 0 || c.Seed == 0 {
		return nil, fmt.Errorf("itofin: invalid GJR dimensions or zero seed")
	}
	for _, value := range []float64{c.Spot, c.DailyVariance, c.RiskFreeRate, c.DividendYield, c.Omega, c.Alpha, c.Beta, c.Gamma, c.Lambda, c.DaysPerYear, c.Horizon} {
		if math.IsNaN(value) || math.IsInf(value, 0) {
			return nil, fmt.Errorf("itofin: GJR inputs must be finite")
		}
	}
	if c.Spot <= 0 || c.DailyVariance < 0 || c.Omega < 0 || c.Alpha < 0 || c.Beta < 0 || c.Alpha+c.Gamma < 0 || c.DaysPerYear <= 0 || c.Horizon < 0 || c.Scheme < GJRPartialTruncation || c.Scheme > GJRReflection {
		return nil, fmt.Errorf("itofin: invalid GJR parameter domain")
	}
	work, err := checkedProduct(c.Steps, c.Paths)
	if err != nil {
		return nil, err
	}
	if _, err := checkedProduct(work, 2); err != nil {
		return nil, err
	}
	times := 1
	if !c.TerminalOnly {
		if c.Steps == int(^uint(0)>>1) {
			return nil, fmt.Errorf("itofin: step count overflow")
		}
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
	input := C.ItofinGjrInput{
		spot: C.double(c.Spot), daily_variance: C.double(c.DailyVariance),
		risk_free_rate: C.double(c.RiskFreeRate), dividend_yield: C.double(c.DividendYield),
		omega: C.double(c.Omega), alpha: C.double(c.Alpha), beta: C.double(c.Beta),
		gamma: C.double(c.Gamma), lambda: C.double(c.Lambda), days_per_year: C.double(c.DaysPerYear),
		horizon: C.double(c.Horizon), steps: C.size_t(c.Steps), paths: C.size_t(c.Paths),
		seed: C.uint32_t(c.Seed), discretization: C.int32_t(c.Scheme), terminal_only: terminal,
	}
	out := make([]float64, count)
	var e C.ItofinError
	if err := ffiError(C.itofin_gjr_paths(&input, doubles(out), C.size_t(len(out)), &e), &e); err != nil {
		return nil, err
	}
	return &Simulation{Values: out, Paths: c.Paths, Times: times, Assets: 2, TerminalOnly: c.TerminalOnly}, nil
}
