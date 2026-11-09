package itofin

/*
#include "itofin.h"
*/
import "C"

// Quote is a read-only session quote: a *SimpleQuote, *DerivedQuote or
// *CompositeQuote. Inputs taking a Quote observe it, so changing a source
// SimpleQuote reprices them.
type Quote interface {
	Value() (float64, error)
	quoteObject() object
}

// DerivedQuote is worth a*source+b and follows its source. It is read-only.
type DerivedQuote struct{ object }

// CompositeQuote combines two quotes with a CompositeOp and follows both. It
// is read-only.
type CompositeQuote struct{ object }

// CompositeOp selects the CompositeQuote function. Divide and power are not
// bound; a square is CompositeMultiply with the same quote twice.
type CompositeOp int32

const (
	CompositeAdd      CompositeOp = C.ITOFIN_COMPOSITE_ADD
	CompositeSubtract CompositeOp = C.ITOFIN_COMPOSITE_SUBTRACT
	CompositeMultiply CompositeOp = C.ITOFIN_COMPOSITE_MULTIPLY
)

func (q *SimpleQuote) quoteObject() object {
	if q == nil {
		return object{}
	}
	return q.object
}
func (q *DerivedQuote) quoteObject() object {
	if q == nil {
		return object{}
	}
	return q.object
}
func (q *CompositeQuote) quoteObject() object {
	if q == nil {
		return object{}
	}
	return q.object
}

func quoteValue(o object) (float64, error) {
	var v C.double
	err := o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_quote_value(o.session.ctx, C.uint64_t(o.id), &v, &e), &e)
	})
	return float64(v), err
}

// Value returns a*source+b for the source's current value.
func (q *DerivedQuote) Value() (float64, error) { return quoteValue(q.object) }

// Value returns op(q1, q2) for the sources' current values.
func (q *CompositeQuote) Value() (float64, error) { return quoteValue(q.object) }

// NewDerivedQuote builds a quote worth a*source+b; source may itself be a
// dependent quote.
func (s *Session) NewDerivedQuote(source Quote, a, b float64) (*DerivedQuote, error) {
	if source == nil {
		return nil, errNilArgument("source quote")
	}
	if err := sameSession(s, source.quoteObject()); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_derived_quote_new(s.ctx, C.uint64_t(source.quoteObject().id), C.double(a), C.double(b), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &DerivedQuote{object{s, uint64(id)}}, nil
}

// NewCompositeQuote builds a quote worth op(q1, q2); CompositeSubtract is
// q1-q2. An unknown op is an error.
func (s *Session) NewCompositeQuote(q1, q2 Quote, op CompositeOp) (*CompositeQuote, error) {
	if q1 == nil || q2 == nil {
		return nil, errNilArgument("source quote")
	}
	if err := sameSession(s, q1.quoteObject(), q2.quoteObject()); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_composite_quote_new(s.ctx, C.uint64_t(q1.quoteObject().id), C.uint64_t(q2.quoteObject().id), C.int32_t(op), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CompositeQuote{object{s, uint64(id)}}, nil
}
