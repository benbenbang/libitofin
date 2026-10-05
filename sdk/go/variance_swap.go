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

// VarianceSwap is a spot-start variance-unit contract with retained settings.
type VarianceSwap struct{ object }

// ReplicatingVarianceSwapEngine uses finite piecewise-linear option strips.
type ReplicatingVarianceSwapEngine struct{ object }

// VarianceSwapConfig uses variance, not volatility, for Strike and positive Notional.
type VarianceSwapConfig struct {
	Position                Position
	Strike, Notional        float64
	StartDate, MaturityDate Date
	Settings                *Settings
}

// ReplicatingVarianceSwapEngineConfig requires an explicitly positive Dk.
// Both strips contain 2..4096 raw positive finite strikes and share a boundary.
type ReplicatingVarianceSwapEngineConfig struct {
	Process                 *BlackScholesProcess
	Dk                      float64
	CallStrikes, PutStrikes []float64
}

// VarianceSwapOptionWeight is a fresh snapshot of a finite replication weight.
type VarianceSwapOptionWeight struct {
	OptionType     OptionType
	Strike, Weight float64
}

func (s *Session) NewVarianceSwap(a VarianceSwapConfig) (*VarianceSwap, error) {
	if a.Settings == nil {
		return nil, errNilArgument("settings")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_variance_swap_new(s.ctx, C.int32_t(a.Position), C.double(a.Strike), C.double(a.Notional), C.int32_t(a.StartDate.Serial()), C.int32_t(a.MaturityDate.Serial()), C.uint64_t(a.Settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VarianceSwap{object{s, uint64(id)}}, nil
}

func (s *Session) NewReplicatingVarianceSwapEngine(a ReplicatingVarianceSwapEngineConfig) (*ReplicatingVarianceSwapEngine, error) {
	if a.Process == nil {
		return nil, errNilArgument("process")
	}
	if len(a.CallStrikes) < 2 || len(a.CallStrikes) > 4096 || len(a.PutStrikes) < 2 || len(a.PutStrikes) > 4096 {
		return nil, fmt.Errorf("itofin: variance-swap strip length out of bounds")
	}
	if !(a.Dk > 0) || math.IsInf(a.Dk, 0) {
		return nil, fmt.Errorf("itofin: variance-swap Dk must be finite and positive")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Process.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_replicating_variance_swap_engine_new(s.ctx, C.uint64_t(a.Process.id), C.double(a.Dk), (*C.double)(unsafe.Pointer(&a.CallStrikes[0])), C.size_t(len(a.CallStrikes)), (*C.double)(unsafe.Pointer(&a.PutStrikes[0])), C.size_t(len(a.PutStrikes)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ReplicatingVarianceSwapEngine{object{s, uint64(id)}}, nil
}

func (v *VarianceSwap) SetEngine(engine *ReplicatingVarianceSwapEngine) error {
	if v == nil || engine == nil {
		return errNilArgument("variance swap or engine")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object, engine.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_variance_swap_set_engine(s.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e)
	})
}

func (v *VarianceSwap) value(field int32) (float64, error) {
	if v == nil {
		return 0, errNilArgument("variance swap")
	}
	var out C.double
	s := v.session
	err := s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_variance_swap_value(s.ctx, C.uint64_t(v.id), C.int32_t(field), &out, &e), &e)
	})
	return float64(out), err
}
func (v *VarianceSwap) NPV() (float64, error)      { return v.value(0) }
func (v *VarianceSwap) Variance() (float64, error) { return v.value(1) }
func (v *VarianceSwap) Strike() (float64, error)   { return v.value(2) }
func (v *VarianceSwap) Notional() (float64, error) { return v.value(3) }

func (v *VarianceSwap) integer(field int32) (int32, error) {
	if v == nil {
		return 0, errNilArgument("variance swap")
	}
	var out C.int32_t
	s := v.session
	err := s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_variance_swap_integer(s.ctx, C.uint64_t(v.id), C.int32_t(field), &out, &e), &e)
	})
	return int32(out), err
}
func (v *VarianceSwap) Position() (Position, error) { x, e := v.integer(0); return Position(x), e }
func (v *VarianceSwap) StartDate() (Date, error) {
	x, e := v.integer(1)
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(x)
}
func (v *VarianceSwap) MaturityDate() (Date, error) {
	x, e := v.integer(2)
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(x)
}
func (v *VarianceSwap) IsCalculated() (bool, error) { x, e := v.integer(3); return x != 0, e }
func (v *VarianceSwap) IsExpired() (bool, error)    { x, e := v.integer(4); return x != 0, e }

func (v *VarianceSwap) Recalculate() error {
	if v == nil {
		return errNilArgument("variance swap")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_variance_swap_recalculate(s.ctx, C.uint64_t(v.id), &e), &e)
	})
}

// OptionWeights returns independent typed values. Count and copy are serialized
// together, including concurrent settings updates, engine replacement and Close.
func (v *VarianceSwap) OptionWeights() ([]VarianceSwapOptionWeight, error) {
	if v == nil {
		return nil, errNilArgument("variance swap")
	}
	var result []VarianceSwapOptionWeight
	s := v.session
	err := s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var count C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_variance_swap_weights_count(s.ctx, C.uint64_t(v.id), &count, &e), &e); err != nil {
			return err
		}
		if count > 8192 {
			return fmt.Errorf("itofin: variance-swap weight count out of bounds")
		}
		n := int(count)
		result = make([]VarianceSwapOptionWeight, n)
		kinds := make([]C.int32_t, n)
		strikes, weights := make([]C.double, n), make([]C.double, n)
		var kp *C.int32_t
		var sp, wp *C.double
		if n > 0 {
			kp, sp, wp = &kinds[0], &strikes[0], &weights[0]
		}
		if err := ffiError(C.itofin_variance_swap_weights(s.ctx, C.uint64_t(v.id), kp, sp, wp, count, &e), &e); err != nil {
			return err
		}
		for i := range result {
			result[i] = VarianceSwapOptionWeight{OptionType(kinds[i]), float64(strikes[i]), float64(weights[i])}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	return result, nil
}
