package itofin

/*
#include "itofin.h"
*/
import "C"

// ParameterSensitivity holds one parameter's derivatives with respect to a
// quote bump. First is nil for a skipped parameter (no reference value);
// Second is nil for a skipped parameter or a OneSide scheme.
type ParameterSensitivity struct {
	First  *float64
	Second *float64
}

func parameterSensitivity(out C.ItofinParameterSensitivity) ParameterSensitivity {
	var result ParameterSensitivity
	if out.has_first != 0 {
		first := float64(out.first)
		result.First = &first
	}
	if out.has_second != 0 {
		second := float64(out.second)
		result.Second = &second
	}
	return result
}

func parameterIDs(s *Session, quotes []*SimpleQuote, parameters []Quote) ([]C.uint64_t, []C.uint64_t, error) {
	objects := make([]object, 0, len(quotes)+len(parameters))
	for _, q := range quotes {
		if q == nil {
			return nil, nil, errNilArgument("quote")
		}
		objects = append(objects, q.object)
	}
	for _, p := range parameters {
		if p == nil {
			return nil, nil, errNilArgument("parameter")
		}
		objects = append(objects, p.quoteObject())
	}
	ids := make([]C.uint64_t, len(objects))
	for i, o := range objects {
		ids[i] = C.uint64_t(o.id)
	}
	return ids[:len(quotes)], ids[len(quotes):], sameSession(s, objects...)
}

// ParameterSensitivity bumps quote by shift and returns the derivatives of
// each parameter, index-aligned with parameters. Parameters may be any Quote,
// typically DerivedQuote or CompositeQuote built on quote. A nil
// referenceValues reads the reference values from the parameters before the
// bump; otherwise it must have one entry per parameter, and a nil entry skips
// its parameter. A zero shift is an error. The quote is restored on success
// and on error.
//
// There is no QuantLib test-suite oracle: the parameter forms were removed
// upstream. An invalid parameter would also be skipped, but no binding can
// build an invalid quote.
func (s *Session) ParameterSensitivity(quote *SimpleQuote, parameters []Quote, referenceValues []*float64, shift float64, kind SensitivityAnalysis) ([]ParameterSensitivity, error) {
	q, p, err := parameterIDs(s, []*SimpleQuote{quote}, parameters)
	if err != nil {
		return nil, err
	}
	var values *C.double
	var has *C.uint8_t
	if referenceValues != nil {
		v := make([]C.double, max(len(referenceValues), 1))
		h := make([]C.uint8_t, len(v))
		for i, r := range referenceValues {
			if r != nil {
				v[i], h[i] = C.double(*r), 1
			}
		}
		values, has = &v[0], &h[0]
	}
	out := make([]C.ItofinParameterSensitivity, max(len(p), 1))
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_parameter_sensitivity(s.ctx, q[0], idPointer(p), C.size_t(len(p)), values, has, C.size_t(len(referenceValues)), C.double(shift), C.int32_t(kind), &out[0], &e), &e)
	})
	if err != nil {
		return nil, err
	}
	results := make([]ParameterSensitivity, len(p))
	for k := range results {
		results[k] = parameterSensitivity(out[k])
	}
	return results, nil
}

// ParameterSensitivityVector is ParameterSensitivity for each quote, bumped
// one at a time, with the reference values read once from the parameters.
// Returns one row per quote, each index-aligned with parameters. An empty
// quotes slice is an error.
func (s *Session) ParameterSensitivityVector(quotes []*SimpleQuote, parameters []Quote, shift float64, kind SensitivityAnalysis) ([][]ParameterSensitivity, error) {
	q, p, err := parameterIDs(s, quotes, parameters)
	if err != nil {
		return nil, err
	}
	out := make([]C.ItofinParameterSensitivity, max(len(q)*len(p), 1))
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_parameter_sensitivity_vector(s.ctx, idPointer(q), C.size_t(len(q)), idPointer(p), C.size_t(len(p)), C.double(shift), C.int32_t(kind), &out[0], &e), &e)
	})
	if err != nil {
		return nil, err
	}
	results := make([][]ParameterSensitivity, len(q))
	for r := range results {
		results[r] = make([]ParameterSensitivity, len(p))
		for c := range results[r] {
			results[r][c] = parameterSensitivity(out[r*len(p)+c])
		}
	}
	return results, nil
}
