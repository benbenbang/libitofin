package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
	"unsafe"
)

// DefaultMaxOutputValues bounds convenience allocations to 128 MiB of doubles.
// Native computation also uses an equally sized temporary result. Use terminal
// mode or smaller batches for large simulations, or explicitly raise the limit.
const DefaultMaxOutputValues = 16 * 1024 * 1024

func doubles(v []float64) *C.double {
	if len(v) == 0 {
		return nil
	}
	return (*C.double)(unsafe.Pointer(&v[0]))
}

// GaussianDraws returns a batch of standard normals from the native MT19937 /
// inverse-normal generator. The nonzero seed deterministically identifies the
// stream; each call starts from that seed. Up to DefaultMaxOutputValues per call.
func GaussianDraws(count int, seed uint32) ([]float64, error) {
	if count < 0 || count > DefaultMaxOutputValues || seed == 0 {
		return nil, fmt.Errorf("itofin: invalid count or zero seed")
	}
	out := make([]float64, count)
	var e C.ItofinError
	err := ffiError(C.itofin_gaussian_draws(C.size_t(count), C.uint32_t(seed), doubles(out), C.size_t(len(out)), &e), &e)
	if err != nil {
		return nil, err
	}
	return out, nil
}

// GBMConfig supplies annualized arithmetic drifts and volatilities. Correlation
// is row-major and positive definite; nil means independent assets. Seed must
// be nonzero. Input slices must not be mutated while SimulateGBM is running.
type GBMConfig struct {
	Initial, Drift, Volatility []float64
	Correlation                []float64
	Horizon                    float64
	Steps, Paths               int
	Seed                       uint32
	TerminalOnly               bool
	// Zero selects DefaultMaxOutputValues; positive values override that limit.
	MaxOutputValues int
}

// Simulation contains row-major [path,time,asset] values, including the exact
// initial values. Terminal-only output has one time row at the horizon.
type Simulation struct {
	Values               []float64
	Paths, Times, Assets int
	TerminalOnly         bool
}

func checkedProduct(a, b int) (int, error) {
	if a < 0 || b < 0 || (a != 0 && b > int(^uint(0)>>1)/a) {
		return 0, fmt.Errorf("itofin: dimension overflow")
	}
	return a * b, nil
}

// SimulateGBM computes an entire batch through one native call. Independent
// calls can run concurrently without a Session. Draw order is path/time/asset.
func SimulateGBM(c GBMConfig) (*Simulation, error) {
	n := len(c.Initial)
	if n < 1 || n > 1024 || len(c.Drift) != n || len(c.Volatility) != n || c.Steps <= 0 || c.Paths <= 0 || c.Seed == 0 {
		return nil, fmt.Errorf("itofin: invalid GBM dimensions or zero seed")
	}
	if math.IsNaN(c.Horizon) || math.IsInf(c.Horizon, 0) || c.Horizon < 0 {
		return nil, fmt.Errorf("itofin: invalid horizon")
	}
	for i := range n {
		if c.Initial[i] <= 0 || math.IsNaN(c.Initial[i]) || math.IsInf(c.Initial[i], 0) || math.IsNaN(c.Drift[i]) || math.IsInf(c.Drift[i], 0) || c.Volatility[i] < 0 || math.IsNaN(c.Volatility[i]) || math.IsInf(c.Volatility[i], 0) {
			return nil, fmt.Errorf("itofin: invalid asset input at %d", i)
		}
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
	count, err = checkedProduct(count, n)
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
	corr := c.Correlation
	if corr == nil {
		corr = make([]float64, n*n)
		for i := range n {
			corr[i*n+i] = 1
		}
	}
	if len(corr) != n*n {
		return nil, fmt.Errorf("itofin: correlation shape mismatch")
	}
	out := make([]float64, count)
	var terminal C.int32_t
	if c.TerminalOnly {
		terminal = 1
	}
	input := C.ItofinGbmInput{initial: doubles(c.Initial), drift: doubles(c.Drift), volatility: doubles(c.Volatility), correlation: doubles(corr), assets: C.size_t(n), horizon: C.double(c.Horizon), steps: C.size_t(c.Steps), paths: C.size_t(c.Paths), seed: C.uint32_t(c.Seed), terminal_only: terminal}
	var e C.ItofinError
	// The C record contains Go pointers. Pin all referenced backing arrays for
	// this synchronous call; none is retained by the native library.
	var pin runtimePinner
	pin.pin(c.Initial, c.Drift, c.Volatility, corr)
	defer pin.unpin()
	err = ffiError(C.itofin_gbm_paths(&input, doubles(out), C.size_t(len(out)), &e), &e)
	if err != nil {
		return nil, err
	}
	return &Simulation{Values: out, Paths: c.Paths, Times: times, Assets: n, TerminalOnly: c.TerminalOnly}, nil
}
