package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
)

// OUConfig supplies a scalar exact Ornstein-Uhlenbeck simulation.
// Each path consumes one normal per step from the seeded native stream.
type OUConfig struct {
	Initial, Level, Speed, Volatility, Horizon float64
	Steps, Paths                               int
	Seed                                       uint32
	TerminalOnly                               bool
	// Zero selects DefaultMaxOutputValues; positive values override that limit.
	MaxOutputValues int
}

// SimulateOU returns row-major [path,time] values including time zero.
// Terminal mode contains one value per path at the horizon.
func SimulateOU(c OUConfig) (*Simulation, error) {
	if math.IsNaN(c.Initial) || math.IsInf(c.Initial, 0) ||
		math.IsNaN(c.Level) || math.IsInf(c.Level, 0) ||
		math.IsNaN(c.Speed) || math.IsInf(c.Speed, 0) || c.Speed < 0 ||
		math.IsNaN(c.Volatility) || math.IsInf(c.Volatility, 0) || c.Volatility < 0 ||
		math.IsNaN(c.Horizon) || math.IsInf(c.Horizon, 0) || c.Horizon < 0 ||
		c.Steps <= 0 || c.Paths <= 0 || c.Seed == 0 {
		return nil, fmt.Errorf("itofin: invalid OU input")
	}
	if _, err := checkedProduct(c.Steps, c.Paths); err != nil {
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
	limit := c.MaxOutputValues
	if limit == 0 {
		limit = DefaultMaxOutputValues
	}
	if limit < 0 || count > limit || count > int(^uint(0)>>1)/8 {
		return nil, fmt.Errorf("itofin: result exceeds output limit; use terminal mode or smaller batches")
	}
	out := make([]float64, count)
	var terminal C.int32_t
	if c.TerminalOnly {
		terminal = 1
	}
	input := C.ItofinOuInput{initial: C.double(c.Initial), level: C.double(c.Level), speed: C.double(c.Speed), volatility: C.double(c.Volatility), horizon: C.double(c.Horizon), steps: C.size_t(c.Steps), paths: C.size_t(c.Paths), seed: C.uint32_t(c.Seed), terminal_only: terminal}
	var nativeErr C.ItofinError
	if err := ffiError(C.itofin_ou_paths(&input, doubles(out), C.size_t(len(out)), &nativeErr), &nativeErr); err != nil {
		return nil, err
	}
	return &Simulation{Values: out, Paths: c.Paths, Times: times, Assets: 1, TerminalOnly: c.TerminalOnly}, nil
}
