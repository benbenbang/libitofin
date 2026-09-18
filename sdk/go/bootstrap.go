package itofin

/*
#include "itofin.h"
typedef int32_t (*goBootstrapFn)(size_t, const ItofinBootstrapState*, ItofinBootstrapOutput*, ItofinError*);
typedef void (*goBootstrapDrop)(size_t);
extern int32_t goBootstrapPenalty(uintptr_t, const ItofinBootstrapState*, ItofinBootstrapOutput*, ItofinError*);
extern int32_t goBootstrapDates(uintptr_t, const ItofinBootstrapState*, ItofinBootstrapOutput*, ItofinError*);
extern void goBootstrapRelease(uintptr_t);
*/
import "C"

import (
	"runtime/cgo"
	"unsafe"
)

// SimpleQuoteVariables retains quotes solved jointly with global curve nodes.
type SimpleQuoteVariables struct{ object }

// NewSimpleQuoteVariables accepts optional guesses and lower bounds, each no
// longer than quotes. Missing guesses are zero; bounds must be below guesses.
func (s *Session) NewSimpleQuoteVariables(quotes []*SimpleQuote, guesses, bounds []float64) (*SimpleQuoteVariables, error) {
	ids := make([]C.uint64_t, len(quotes))
	for i, q := range quotes {
		if q == nil {
			return nil, errNilArgument("quote")
		}
		if err := sameSession(s, q.object); err != nil {
			return nil, err
		}
		ids[i] = C.uint64_t(q.id)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_simple_quote_variables_new(s.ctx, unsafe.SliceData(ids), C.size_t(len(ids)), (*C.double)(unsafe.Pointer(unsafe.SliceData(guesses))), C.size_t(len(guesses)), (*C.double)(unsafe.Pointer(unsafe.SliceData(bounds))), C.size_t(len(bounds)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SimpleQuoteVariables{object{s, uint64(id)}}, nil
}

// BootstrapState contains independent copies of the current trial discount
// curve nodes. Times use the curve day counter; Data holds discount factors,
// interpolated using the configured LogLinear or Linear interpolation.
// QuoteValues follow AdditionalVariables; HelperErrors follow AdditionalHelpers.
// Callbacks run synchronously on the session worker. Session calls, including
// Close and calls from other goroutines during a callback, return ErrCallbackReentry.
// Use these snapshots instead of querying or mutating session objects.
type BootstrapState struct{ Times, Data, QuoteValues, HelperErrors []float64 }

type bootstrapCallbacks struct {
	session   *Session
	penalties func(BootstrapState) ([]float64, error)
	dates     func() ([]Date, error)
}

func (s *Session) globalCurve(cfg PiecewiseCurveConfig, kind int, ids, extra []C.uint64_t, out *C.uint64_t) error {
	var variables C.uint64_t
	if cfg.AdditionalVariables != nil {
		if err := sameSession(s, cfg.AdditionalVariables.object); err != nil {
			return err
		}
		variables = C.uint64_t(cfg.AdditionalVariables.id)
	}
	handle := cgo.NewHandle(bootstrapCallbacks{s, cfg.AdditionalPenalties, cfg.AdditionalDates})
	var adopted C.bool
	defer func() {
		if !bool(adopted) {
			handle.Delete()
		}
	}()
	callbacks := C.ItofinBootstrapCallbacks{userdata: C.size_t(handle), release: (C.goBootstrapDrop)(C.goBootstrapRelease)}
	if cfg.AdditionalPenalties != nil {
		callbacks.penalties = (C.goBootstrapFn)(C.goBootstrapPenalty)
	}
	if cfg.AdditionalDates != nil {
		callbacks.dates = (C.goBootstrapFn)(C.goBootstrapDates)
	}
	var e C.ItofinError
	return ffiError(C.itofin_global_curve_new(s.ctx, C.int32_t(cfg.ReferenceDate.Serial()), unsafe.SliceData(ids), C.size_t(len(ids)), C.uint64_t(cfg.DayCounter.id), C.int32_t(kind), unsafe.SliceData(extra), C.size_t(len(extra)), variables, &callbacks, &adopted, out, &e), &e)
}
