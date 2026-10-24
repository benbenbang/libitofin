package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// BPSFromCurve returns coupon-rate sensitivity per 100, not signed yield BPV.
// Coupon amounts are not read; settlement-date and ex-coupon flows are excluded.
func (b *Bond) BPSFromCurve(discount *YieldTermStructure, settlement *Date) (float64, error) {
	if discount == nil {
		return 0, fmt.Errorf("discount curve required")
	}
	serial, err := bondDate(settlement)
	if err != nil {
		return 0, err
	}
	return b.analytics([]object{discount.object}, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_bps_from_curve(ctx, id, C.uint64_t(discount.id), serial, out, e)
	})
}

// BPSFromYield returns coupon-rate sensitivity per 100 under flat-yield discounting.
// It does not read coupon amounts and is not the signed yield BasisPointValue.
func (b *Bond) BPSFromYield(rate float64, a BondYieldConfig) (float64, error) {
	c, objects, err := yieldAnalyticsConfig(a)
	if err != nil {
		return 0, err
	}
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_bps_from_yield(ctx, id, C.double(rate), c, out, e)
	})
}

// ATMRateFromCurve returns the coupon rate repricing the bond's own curve NPV.
// It is not a par-price target. Ordinary fixed, zero and floating bonds are supported.
func (b *Bond) ATMRateFromCurve(discount *YieldTermStructure, settlement *Date) (float64, error) {
	if discount == nil {
		return 0, fmt.Errorf("discount curve required")
	}
	serial, err := bondDate(settlement)
	if err != nil {
		return 0, err
	}
	return b.analytics([]object{discount.object}, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_atm_rate_from_curve(ctx, id, C.uint64_t(discount.id), serial, out, e)
	})
}

// ATMRateFromPrice returns the coupon rate repricing an explicit per-100 quote.
// Clean quotes include accrued interest; scaling uses current settlement notional.
// Live coupon amounts are read, even for Dirty. The own-NPV ATM method is unchanged.
func (b *Bond) ATMRateFromPrice(discount *YieldTermStructure, price float64, priceType BondPriceType, settlement *Date) (float64, error) {
	if discount == nil {
		return 0, fmt.Errorf("discount curve required")
	}
	if priceType != Clean && priceType != Dirty {
		return 0, fmt.Errorf("invalid bond price type")
	}
	serial, err := bondDate(settlement)
	if err != nil {
		return 0, err
	}
	return b.analytics([]object{discount.object}, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_atm_rate_from_price(ctx, id, C.uint64_t(discount.id), C.double(price), C.int32_t(priceType), serial, out, e)
	})
}
