# Fixed and zero-coupon bond bindings

Go and Python expose the existing Rust fixed-rate and zero-coupon bonds through
one shared `Bond` interface. This is a foundation slice, not complete bond parity:
floating/callable bonds, yield inversion, Z-spreads and analytical risk measures
remain separate work.

## Python

```python
from itofin import Settings
from itofin.instruments import FixedRateBond, ZeroCouponBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule

settings = Settings()
settings.set_evaluation_date(Date(15, 3, 2026))
calendar = Calendar.null_calendar()
day_counter = DayCounter.actual360()
curve = FlatForward(Date(15, 3, 2026), 0.031, day_counter)
engine = DiscountingBondEngine(curve, settings)
schedule = Schedule(
    Date(15, 1, 2026), Date(15, 1, 2030), Frequency.Annual,
    calendar, BusinessDayConvention.Unadjusted,
)
bond = FixedRateBond(2, 250.0, schedule, [0.045], day_counter, settings)
bond.set_engine(engine)
print(bond.npv(), bond.clean_price(), bond.dirty_price())
zero = ZeroCouponBond(2, calendar, 250.0, Date(17, 1, 2027), settings)
zero.set_engine(engine)
print(zero.settlement_value(), zero.settlement_date())
```

`FixedRateBond` and `ZeroCouponBond` inherit a nonconstructible `Bond` base.
Constructors require explicit settings and retain them. `DiscountingBondEngine`
retains the curve and settings; attaching an engine with different settings fails.
Python objects follow the existing thread-confined facade contract.

Fixed bonds accept coupon vectors, payment convention/calendar, redemption,
issue date, ex-coupon period/calendar/convention/end-of-month and a first-period
day counter. Omitted payment calendar uses the schedule's calendar. Defaults are
Following, redemption 100, no issue/ex-coupon/first-period override, Unadjusted
ex-coupon convention and false ex-coupon end-of-month. Explicit zero ex-coupon
period retains the Rust meaning: ex-coupon on the payment date.

## Go

Run `go run ./examples/bonds` from `sdk/go` after building the C library.

```go
package main

import (
	"errors"
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
	"log"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}
func run() (err error) {
	s, err := itofin.NewSession()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, s.Close()) }()
	today, err := itofin.NewDate(15, 1, 2024)
	if err != nil {
		return err
	}
	maturity, err := itofin.NewDate(15, 1, 2027)
	if err != nil {
		return err
	}
	settings, err := s.NewSettings()
	if err != nil {
		return err
	}
	if err = settings.SetEvaluationDate(today); err != nil {
		return err
	}
	calendar, err := s.NullCalendar()
	if err != nil {
		return err
	}
	counter, err := s.Actual365Fixed()
	if err != nil {
		return err
	}
	curve, err := s.NewFlatForward(today, .04, counter)
	if err != nil {
		return err
	}
	engine, err := s.NewDiscountingBondEngine(curve, settings, nil)
	if err != nil {
		return err
	}
	bond, err := s.NewZeroCouponBond(itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Calendar: calendar, Settings: settings, IssueDate: &today}, MaturityDate: maturity})
	if err != nil {
		return err
	}
	if err = bond.SetEngine(engine); err != nil {
		return err
	}
	price, err := bond.CleanPrice()
	if err != nil {
		return err
	}
	fmt.Printf("Zero-coupon clean price per 100: %.12f\n", price)
	return nil
}
```

Expected clean price per 100: `88.701763880339`.

## Common valuation and ownership

| Query | Meaning |
| --- | --- |
| NPV | Currency value discounted to the curve reference date |
| Settlement value | Dirty currency value at settlement |
| Clean/dirty price | Quote per 100 of outstanding notional |
| Accrued amount | Accrued interest per 100, optionally at an explicit date |
| Notional | Outstanding currency amount, optionally at an explicit date |
| Settlement date | Evaluation/explicit date advanced by business-day lag, floored at issue |
| Maturity date | Contractual maturity, potentially different from the payment date |
| Cashflows/redemptions | Retained legs; flows survive instrument-wrapper disposal |

Engines observe retained market dependencies. Quote and evaluation-date changes
invalidate prices lazily. The new bond facades also refresh cached prices after
changes to the settings' reference-date event and today's cash-flow flags.
`freeze` pins cached results for unexpired instruments; native/core expiry
bypasses freeze and still returns zero. `recalculate` refreshes results while
keeping the bond frozen; `unfreeze` restores live pricing. Quotes still use the current
notional and accrual rules. Freezing before calculation does not manufacture a
price and returns a missing-result error.

`include_settlement_date_flows=None` lets the engine use the settings'
reference-date event policy. An explicit override applies to reference-date
valuation. Settlement-date cash flows are normally excluded, but the existing
core today's-cash-flow override takes precedence on the evaluation date.
Expiry, zero-notional prices and adjusted payment dates retain the Rust rules.
Call `Recalculate`/`recalculate` after silent calendar holiday mutations when a
new settlement date must refresh an already cached settlement value. These
facades do not change the existing core calendar notification contract.

Face amount and redemption must be positive finite values. Coupon rates and
resulting monetary amounts must be finite. Date/holiday-table boundaries and
settlement overflows raise ordinary binding errors instead of native panics.
Dirty quotes keep the core arithmetic order for ordinary inputs and divide
currency value by notional before multiplying by 100 when the original scaling
overflows, keeping representable quotes usable with very large face amounts.

## Verification

Tracked Python native fixtures were generated independently with QuantLib 1.43.
They cover fixed/step-up/ex-coupon legs, non-par redemption, adjusted zero-coupon
payments and expiry. Additional tests cover dependency retention, changing
quotes/settings, freeze/recalculate/unfreeze, extreme scales and invalid inputs.
Generated Python stubs and the C/Go coverage inventory remain required gates.
