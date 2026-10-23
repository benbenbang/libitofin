package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// DurationType selects the flat-yield duration convention for ordinary bond cash flows.
type DurationType int32

const (
	// SimpleDuration is the discounted cash-flow-weighted average payment time.
	SimpleDuration DurationType = 0
	// MacaulayDuration scales modified duration by 1 + yield/frequency and requires Compounded.
	MacaulayDuration DurationType = 1
	// ModifiedDuration is the negative first yield derivative divided by bond value.
	ModifiedDuration DurationType = 2
)

func (b *Bond) risk(field int32, rate float64, a BondYieldConfig) (float64, error) {
	c, objects, err := yieldAnalyticsConfig(a)
	if err != nil {
		return 0, err
	}
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_risk(ctx, id, C.int32_t(field), C.double(rate), c, out, e)
	})
}

// Duration returns flat-yield duration without a pricing engine; Macaulay requires Compounded.
func (b *Bond) Duration(rate float64, kind DurationType, a BondYieldConfig) (float64, error) {
	if kind < SimpleDuration || kind > ModifiedDuration {
		return 0, fmt.Errorf("invalid duration type")
	}
	return b.risk(int32(kind), rate, a)
}

// Convexity returns the flat-yield second derivative divided by ordinary bond value.
func (b *Bond) Convexity(rate float64, a BondYieldConfig) (float64, error) {
	return b.risk(3, rate, a)
}

// BasisPointValue returns signed, unscaled currency value for a +1 bp yield change.
// It is not a per-100 quote and retains the native convexity/100 correction.
func (b *Bond) BasisPointValue(rate float64, a BondYieldConfig) (float64, error) {
	return b.risk(4, rate, a)
}

// YieldValueBasisPoint returns the yield change for a +0.01 currency value change.
// It is signed, uses unscaled currency value, and varies inversely with face amount.
func (b *Bond) YieldValueBasisPoint(rate float64, a BondYieldConfig) (float64, error) {
	return b.risk(5, rate, a)
}
