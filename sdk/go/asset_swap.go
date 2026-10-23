package itofin

/*
#include "itofin.h"
*/
import "C"

// AssetSwap exchanges a plain fixed/zero bullet bond for a floating Ibor leg.
type AssetSwap struct{ object }

// AssetSwapConfig defines a full-maturity par swap. A nil FloatingDayCounter uses
// the index day counter. A nil FloatingSchedule generates backwards from bond
// settlement using the index tenor; explicit schedules must reach bond maturity.
type AssetSwapConfig struct {
	Bond               *Bond
	Index              *IborIndex
	FloatingSchedule   *Schedule
	FloatingDayCounter *DayCounter
	CleanPrice, Spread float64
	PayBondCoupon      bool
}

// NewAssetSwap retains the original bond, index and their shared Settings.
func (s *Session) NewAssetSwap(a AssetSwapConfig) (*AssetSwap, error) {
	if a.Bond == nil || a.Index == nil {
		return nil, errNilArgument("asset swap argument")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{a.Bond.object, a.Index.object}
		c := C.ItofinAssetSwapConfig{bond: C.uint64_t(a.Bond.id), index: C.uint64_t(a.Index.id), clean_price: C.double(a.CleanPrice), spread: C.double(a.Spread)}
		if a.FloatingSchedule != nil {
			objects = append(objects, a.FloatingSchedule.object)
			c.floating_schedule = C.uint64_t(a.FloatingSchedule.id)
		}
		if a.PayBondCoupon {
			c.pay_bond_coupon = 1
		}
		if a.FloatingDayCounter != nil {
			objects = append(objects, a.FloatingDayCounter.object)
			c.floating_day_counter = C.uint64_t(a.FloatingDayCounter.id)
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_asset_swap_new(s.ctx, &c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &AssetSwap{object{s, uint64(id)}}, nil
}

// SetEngine attaches a live discount curve using native date and flow defaults.
// Settings must be the same allocation used to construct the swap and indexes.
func (v *AssetSwap) SetEngine(curve *YieldTermStructure, settings *Settings) error {
	if v == nil || curve == nil || settings == nil {
		return errNilArgument("asset swap engine argument")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object, curve.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_asset_swap_set_engine(s.ctx, C.uint64_t(v.id), C.uint64_t(curve.id), C.uint64_t(settings.id), &e), &e)
	})
}
func (v *AssetSwap) value(query int32) (float64, error) {
	if v == nil {
		return 0, errNilArgument("asset swap")
	}
	s := v.session
	var out C.double
	err := s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_asset_swap_value(s.ctx, C.uint64_t(v.id), C.int32_t(query), &out, &e), &e)
	})
	return float64(out), err
}
func (v *AssetSwap) action(action int32) error {
	if v == nil {
		return errNilArgument("asset swap")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_asset_swap_action(s.ctx, C.uint64_t(v.id), C.int32_t(action), &e), &e)
	})
}

// NPV returns the signed total present value, including eligible upfront exchange.
func (v *AssetSwap) NPV() (float64, error) { return v.value(0) }

// BondLegNPV returns the signed bond-leg present value.
func (v *AssetSwap) BondLegNPV() (float64, error) { return v.value(1) }

// FloatingLegNPV returns signed floating-leg value, including principal exchanges.
func (v *AssetSwap) FloatingLegNPV() (float64, error) { return v.value(2) }

// FloatingLegBPS returns signed floating-leg sensitivity for a one-basis-point spread shift.
func (v *AssetSwap) FloatingLegBPS() (float64, error) { return v.value(3) }

// FairSpread returns the absolute floating spread that zeroes current NPV.
func (v *AssetSwap) FairSpread() (float64, error) { return v.value(4) }

// FairCleanPrice is a quote per 100, zeroing NPV only when the upfront is included.
// It fails for seasoned swaps with upfront before the discount curve reference date.
func (v *AssetSwap) FairCleanPrice() (float64, error) { return v.value(5) }

// CleanPrice returns the original quoted clean price per 100.
func (v *AssetSwap) CleanPrice() (float64, error) { return v.value(6) }

// Spread returns the original floating spread in decimal rate units.
func (v *AssetSwap) Spread() (float64, error) { return v.value(7) }

// UpfrontDate returns the date of the dirty-price upfront payment.
func (v *AssetSwap) UpfrontDate() (Date, error) {
	x, e := v.value(8)
	if e != nil {
		return Date{}, e
	}
	return DateFromSerial(int32(x))
}

// PayBondCoupon reports whether bond coupons are paid rather than received.
func (v *AssetSwap) PayBondCoupon() (bool, error) { x, e := v.value(9); return x != 0, e }

// ParSwap reports true for every supported instrument and validates ownership.
func (v *AssetSwap) ParSwap() (bool, error) { _, e := v.value(9); return e == nil, e }

// IsCalculated reads the native cache flag without calculating.
func (v *AssetSwap) IsCalculated() (bool, error) { x, e := v.value(10); return x != 0, e }

// IsExpired tests native expiry without calculating.
func (v *AssetSwap) IsExpired() (bool, error) { x, e := v.value(11); return x != 0, e }

// Calculate populates native cached results if needed.
func (v *AssetSwap) Calculate() error { return v.action(0) }

// Recalculate forces native repricing, including while frozen.
func (v *AssetSwap) Recalculate() error { return v.action(1) }

// Freeze pins native cached results.
func (v *AssetSwap) Freeze() error { return v.action(2) }

// Unfreeze resumes native observer-driven repricing.
func (v *AssetSwap) Unfreeze() error { return v.action(3) }
