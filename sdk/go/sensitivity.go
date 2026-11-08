package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// DefaultShift is the QuantLib default bump, one basis point.
const DefaultShift = 1e-4

// SensitivityAnalysis selects the finite-difference scheme.
type SensitivityAnalysis int32

const (
	// OneSide is a forward difference: first order only.
	OneSide SensitivityAnalysis = iota
	// Centered is a central difference: first and second order.
	Centered
)

// Instrument is a session-owned instrument accepted by sensitivity analysis:
// any bond, a vanilla swap, an overnight indexed swap, a vanilla option, a
// swaption, a cap/floor or a credit default swap. Under a Monte Carlo engine
// each reprice carries sampling error that does not shrink with the shift, so
// the finite differences are noisy; prefer an analytic, tree or
// finite-difference engine.
type Instrument interface {
	instrumentObject() object
}

// Sensitivity holds the first derivative and, for Centered, the second.
type Sensitivity struct {
	First  float64
	Second *float64
}

func (b *Bond) instrumentObject() object {
	if b == nil {
		return object{}
	}
	return b.object
}

func (v *VanillaSwap) instrumentObject() object {
	if v == nil {
		return object{}
	}
	return v.object
}

func (v *OvernightIndexedSwap) instrumentObject() object {
	if v == nil {
		return object{}
	}
	return v.object
}

func (o *VanillaOption) instrumentObject() object {
	if o == nil {
		return object{}
	}
	return o.object
}

func (o *Swaption) instrumentObject() object {
	if o == nil {
		return object{}
	}
	return o.object
}

func (o *CapFloor) instrumentObject() object {
	if o == nil {
		return object{}
	}
	return o.object
}

func (c *CreditDefaultSwap) instrumentObject() object {
	if c == nil {
		return object{}
	}
	return c.object
}

func sensitivityIDs(s *Session, quotes []*SimpleQuote, instruments []Instrument) ([]C.uint64_t, []C.uint64_t, error) {
	objects := make([]object, 0, len(quotes)+len(instruments))
	for _, q := range quotes {
		if q == nil {
			return nil, nil, errNilArgument("quote")
		}
		objects = append(objects, q.object)
	}
	for _, instrument := range instruments {
		if instrument == nil {
			return nil, nil, errNilArgument("instrument")
		}
		objects = append(objects, instrument.instrumentObject())
	}
	ids := make([]C.uint64_t, len(objects))
	for i, o := range objects {
		ids[i] = C.uint64_t(o.id)
	}
	return ids[:len(quotes)], ids[len(quotes):], sameSession(s, objects...)
}

func idPointer(ids []C.uint64_t) *C.uint64_t {
	if len(ids) == 0 {
		return nil
	}
	return &ids[0]
}

func sensitivity(out C.ItofinSensitivity) Sensitivity {
	result := Sensitivity{First: float64(out.first)}
	if out.has_second != 0 {
		second := float64(out.second)
		result.Second = &second
	}
	return result
}

// ParallelAnalysis bumps every quote together by shift and reprices the
// weighted instruments. Empty quantities mean unit weights; a nil referenceNPV
// is computed from the instruments. A zero shift is an error. Quotes are
// restored on success and on error.
func (s *Session) ParallelAnalysis(quotes []*SimpleQuote, instruments []Instrument, quantities []float64, shift float64, kind SensitivityAnalysis, referenceNPV *float64) (Sensitivity, error) {
	q, i, err := sensitivityIDs(s, quotes, instruments)
	if err != nil {
		return Sensitivity{}, err
	}
	var reference *C.double
	if referenceNPV != nil {
		value := C.double(*referenceNPV)
		reference = &value
	}
	var out C.ItofinSensitivity
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_sensitivity_parallel(s.ctx, idPointer(q), C.size_t(len(q)), idPointer(i), C.size_t(len(i)), doubles(quantities), C.size_t(len(quantities)), C.double(shift), C.int32_t(kind), reference, &out, &e), &e)
	})
	if err != nil {
		return Sensitivity{}, err
	}
	return sensitivity(out), nil
}

// BucketAnalysis bumps each quote alone by shift, against one shared reference
// NPV, and returns one Sensitivity per quote in quote order. Same conventions
// as ParallelAnalysis.
func (s *Session) BucketAnalysis(quotes []*SimpleQuote, instruments []Instrument, quantities []float64, shift float64, kind SensitivityAnalysis) ([]Sensitivity, error) {
	q, i, err := sensitivityIDs(s, quotes, instruments)
	if err != nil {
		return nil, err
	}
	out := make([]C.ItofinSensitivity, max(len(q), 1))
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_sensitivity_bucket(s.ctx, idPointer(q), C.size_t(len(q)), idPointer(i), C.size_t(len(i)), doubles(quantities), C.size_t(len(quantities)), C.double(shift), C.int32_t(kind), &out[0], &e), &e)
	})
	if err != nil {
		return nil, err
	}
	results := make([]Sensitivity, len(q))
	for k := range results {
		results[k] = sensitivity(out[k])
	}
	return results, nil
}

// BucketAnalysisMatrix is BucketAnalysis over a ragged matrix of quotes: each
// quote bumped alone against one shared reference NPV, with results shaped
// like quotes. Empty rows are allowed and yield empty rows; an empty outer
// slice is an error. Same conventions as ParallelAnalysis.
func (s *Session) BucketAnalysisMatrix(quotes [][]*SimpleQuote, instruments []Instrument, quantities []float64, shift float64, kind SensitivityAnalysis) ([][]Sensitivity, error) {
	var flat []*SimpleQuote
	lengths := make([]C.size_t, len(quotes))
	for r, row := range quotes {
		flat = append(flat, row...)
		lengths[r] = C.size_t(len(row))
	}
	q, i, err := sensitivityIDs(s, flat, instruments)
	if err != nil {
		return nil, err
	}
	var rows *C.size_t
	if len(lengths) > 0 {
		rows = &lengths[0]
	}
	out := make([]C.ItofinSensitivity, max(len(q), 1))
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_sensitivity_bucket_matrix(s.ctx, idPointer(q), C.size_t(len(q)), rows, C.size_t(len(lengths)), idPointer(i), C.size_t(len(i)), doubles(quantities), C.size_t(len(quantities)), C.double(shift), C.int32_t(kind), &out[0], &e), &e)
	})
	if err != nil {
		return nil, err
	}
	results := make([][]Sensitivity, len(quotes))
	k := 0
	for r, row := range quotes {
		results[r] = make([]Sensitivity, len(row))
		for c := range row {
			results[r][c] = sensitivity(out[k])
			k++
		}
	}
	return results, nil
}

// LabelledQuote names one quote of a key-rate report.
type LabelledQuote struct {
	Label string
	Quote *SimpleQuote
}

// KeyRate is the bucket sensitivity of one labelled quote.
type KeyRate struct {
	Label       string
	Sensitivity Sensitivity
}

// KeyRateAnalysis is BucketAnalysis over labelled quotes, the Go mirror of
// the Python bucket_analysis mapping overload: one KeyRate per quote, in input
// order. Duplicate labels are an error, as mapping keys are unique.
func (s *Session) KeyRateAnalysis(quotes []LabelledQuote, instruments []Instrument, quantities []float64, shift float64, kind SensitivityAnalysis) ([]KeyRate, error) {
	plain := make([]*SimpleQuote, len(quotes))
	seen := make(map[string]struct{}, len(quotes))
	for i, q := range quotes {
		if _, ok := seen[q.Label]; ok {
			return nil, fmt.Errorf("duplicate key-rate label %q", q.Label)
		}
		seen[q.Label] = struct{}{}
		plain[i] = q.Quote
	}
	buckets, err := s.BucketAnalysis(plain, instruments, quantities, shift, kind)
	if err != nil {
		return nil, err
	}
	report := make([]KeyRate, len(buckets))
	for i, b := range buckets {
		report[i] = KeyRate{quotes[i].Label, b}
	}
	return report, nil
}
