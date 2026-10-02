package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
)

// MertonConfig supplies annualized inputs for constant-parameter jump paths.
// Drift is expected arithmetic spot growth including jumps: risk-neutral callers
// supply r-q; real-world callers supply their own forecast assumption.
// Zero MaxOutputValues selects DefaultMaxOutputValues; positive values override.
type MertonConfig struct {
	Spot, Drift, Volatility    float64
	JumpIntensity, LogMeanJump float64
	LogJumpVolatility, Horizon float64
	Steps, Paths               int
	Seed                       uint32
	TerminalOnly               bool
	MaxOutputValues            int
}

// SimulateMerton returns owned row-major [path,time] spot values including time
// zero. Terminal mode equals each full path's final value bit for bit. Calls are
// independent and may run concurrently without a Session. The nonzero seed
// selects separate MT19937 diffusion, event-count and aggregate-jump streams.
func SimulateMerton(c MertonConfig) (*Simulation, error) {
	if c.Steps <= 0 || c.Paths <= 0 || c.Seed == 0 {
		return nil, fmt.Errorf("itofin: invalid Merton dimensions or zero seed")
	}
	for _, value := range []float64{c.Spot, c.Drift, c.Volatility, c.JumpIntensity, c.LogMeanJump, c.LogJumpVolatility, c.Horizon} {
		if math.IsNaN(value) || math.IsInf(value, 0) {
			return nil, fmt.Errorf("itofin: Merton inputs must be finite")
		}
	}
	if c.Spot <= 0 || c.Volatility < 0 || c.JumpIntensity < 0 || c.LogJumpVolatility < 0 || c.Horizon < 0 {
		return nil, fmt.Errorf("itofin: invalid Merton parameter domain")
	}
	work, err := checkedProduct(c.Steps, c.Paths)
	if err != nil {
		return nil, err
	}
	if _, err := checkedProduct(work, 3); err != nil {
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
	var terminal C.int32_t
	if c.TerminalOnly {
		terminal = 1
	}
	input := C.ItofinMertonInput{
		spot: C.double(c.Spot), drift: C.double(c.Drift), volatility: C.double(c.Volatility),
		jump_intensity: C.double(c.JumpIntensity), log_mean_jump: C.double(c.LogMeanJump),
		log_jump_volatility: C.double(c.LogJumpVolatility), horizon: C.double(c.Horizon),
		steps: C.size_t(c.Steps), paths: C.size_t(c.Paths), seed: C.uint32_t(c.Seed), terminal_only: terminal,
	}
	out := make([]float64, count)
	var e C.ItofinError
	if err := ffiError(C.itofin_merton_paths(&input, doubles(out), C.size_t(len(out)), &e), &e); err != nil {
		return nil, err
	}
	return &Simulation{Values: out, Paths: c.Paths, Times: times, Assets: 1, TerminalOnly: c.TerminalOnly}, nil
}
