# QuantLib compatibility

[Project index](../README.md)

The port targets **semantic** faithfulness (matching QuantLib's results), not
bit-for-bit reproduction of its implementation. A small, deliberate set of
divergences is catalogued here. Each is an intentional, reviewed decision (not an
oversight) and is documented at the point of divergence in the source.

## Time / calendars (EPIC-2)

- **Calendar holiday overrides are per-value, not process-global.** QuantLib
  shares one global `Impl` per market, so `addHoliday` on any `TARGET()` handle
  is visible through every other. This port shares added/removed holidays only
  among *clones* of a `Calendar` value, matching the "explicit state, no hidden
  singletons" decision. The built-in holiday rules are identical; only the
  reach of `add_holiday`/`remove_holiday` differs.
- **`holiday_list` filters weekends by a date-aware rule.** QuantLib's
  `holidayList` excludes weekends using the weekday-only `isWeekend`, which
  misclassifies holidays for markets whose weekend changed over time (Saudi
  Arabia's Thu/Fri→Fri/Sat in 2013, Israel/TASE's Fri/Sat→Sat/Sun in 2026). This
  port filters with a date-aware `is_weekend_on`; fixed-weekend calendars are
  unaffected (the default equals the weekday rule).
- **Table-backed calendars fail loudly past their data horizon.** Where QuantLib
  tabulates lunar / religious / observed holidays only up to a fixed year and
  then silently returns "business day" for later dates, this port panics with a
  clear message once a query passes the last fully-tabulated year. QuantLib's
  tables are kept verbatim; we never fabricate future dates.
- **`Period` comparison is a partial order, and fixes a negative-period bug.**
  QuantLib's `operator<`/`operator==` throw when two periods have overlapping
  day ranges (e.g. `1 Month` vs `30 Days`); this port returns `None` from
  `partial_cmp` instead, so comparison never panics. It also orders the day
  bounds `min <= max` before comparing, which QuantLib omits: for negative
  lengths QuantLib's inverted bounds make overlapping periods (like `-1 Month`
  vs `-30 Days`) look decidably ordered, whereas this port correctly reports
  them as undecidable. Positive comparisons are unaffected.
- **`DayCounter` is always valid; there is no empty placeholder.** QuantLib's
  default-constructed `DayCounter` holds a null `impl_` and `QL_REQUIRE`s a
  non-null one on every call. This port omits the empty state, so a
  `DayCounter` always wraps a concrete convention and its accessors never trip
  that null check. The "not yet set" placeholder used by higher layers is an
  `Option<DayCounter>` at those call sites instead.
- **`Business/252` counts directly instead of via a process-global cache.**
  QuantLib memoizes per-month and per-year business-day totals in global
  `std::map`s keyed by calendar name; this port counts with
  `Calendar::business_days_between` directly. With a calendar's built-in
  schedule the results are identical; once holidays are overridden they can
  differ, since QuantLib's name-keyed cache goes stale while this port always
  reflects the current holiday set.
- **`Actual/Actual (ISMA)` uses the reference-date algorithm.** QuantLib picks a
  schedule-driven implementation when a `Schedule` is supplied and a
  reference-date one otherwise; the reference-date path is ported, with the
  schedule-driven overload following as needed.

## Core (EPIC-0)

- **An unset evaluation date is an explicit error, not a system-clock fallback.**
  QuantLib's `Settings` singleton falls back to the machine clock; this core has
  no clock (for determinism and FFI), so operations that need the evaluation date
  return `Err` when it is unset rather than silently pricing a possibly-expired
  instrument as live.
- **Past index fixings live on `Settings`, not in a global `IndexManager`
  singleton (D11).** QuantLib stores fixing history in `IndexManager::instance()`,
  a global-singleton `map<string, TimeSeries<Real>>`; D5 forbids that singleton,
  so the history moves onto `Settings` as a `fixing_store`, exactly like the
  evaluation date. It stays shared across every handle to an index (a `Settings`
  is shared, and the store is keyed by the case-insensitive index name), so two
  handles to "the same" Euribor observe one history, as C++'s global map
  guarantees. Each index name owns an `Observable`, so adding or clearing a
  fixing notifies exactly that index's observers, and those notifiers outlive a
  `clear_fixings`. A conflicting fixing on an existing date returns `Err` (D4)
  where QuantLib notifies and then throws. The `enforces_todays_historic_fixings`
  flag's today's-fixing rule is applied by the index layer that reads the store,
  as `InterestRateIndex::fixing` does in C++, not by the store itself.

## Cash flows (EPIC-7)

- **`Coupon` is not a subtrait of `CashFlow`; one blanket impl derives the
  cash-flow face from the coupon.**
  C++ derives `Coupon` from `CashFlow`, and its base class supplies
  `hasOccurred`, `exCouponDate()` and `isCoupon()` so that no coupon can forget
  them. Rust has no specialization, so under the subtrait shape a `Coupon`
  cannot supply its supertrait's methods on its implementors' behalf. All three
  wrong answers compile and are plausible numbers rather than diagnostics: the
  plain-event `has_occurred` ignores `Settings::includeTodaysCashFlows` on the
  evaluation date; a `None` `ex_coupon_date` accrues ex-coupon while reporting
  `trading_ex_coupon` as `false`; a `None` `as_coupon` contributes nothing to
  `bps`, has its amount subtracted from `atm_rate`'s target, and reports its
  payment date to `maturity_date` in place of its accrual end.

  So `Coupon` restates its own surface (`coupon_base`, `amount`, `rate`,
  `day_counter`, `accrued_amount`, and `AsObservable`), and
  `impl<T: Coupon> CashFlow for T` writes the three exactly once, reading
  `CouponBase`. A coupon cannot get them wrong because it cannot supply them.
  `amount` moves onto `Coupon` for the same reason it must: a blanket that
  *provided* `amount` would give `FixedRateCoupon` a generic body in place of
  its compounded one, and a competing `impl CashFlow for FixedRateCoupon`
  restoring it is a conflicting implementation. The three methods stay required
  on `CashFlow` and `Event` for the flows that are not coupons, which still
  forward explicitly to `event_has_occurred` or `cash_flow_has_occurred`.

  Two costs. `dyn Coupon` does not implement `CashFlow`: the `Some(self)` of
  `as_coupon` is an unsizing coercion, so the blanket takes a `Sized` `T`, and
  `+ ?Sized` would restore the impl and forbid the `Some(self)`. Nothing needs
  it - the `CashFlows` analytics hold a `&dyn CashFlow` already and read only
  `Coupon`'s own methods through the coupon view. And `amount` now sits on both
  traits, so on a concrete coupon with both in scope an unqualified
  `coupon.amount()` is ambiguous; name the trait.

- **The `IborCoupon` par/indexed forecast switch is threaded on `Settings`, not
  a global singleton.** QuantLib's `IborCoupon::indexFixing()` forecast branch
  selects between a par-coupon approximation (rolls `fixingEndDate` off the
  coupon's own accrual end) and an indexed coupon (the index's natural maturity)
  through the `IborCoupon::Settings` singleton (`usingAtParCoupons`, default
  `true`). That process-global switch is what D5 forbids, so the port drops the
  singleton and threads the flag explicitly on `Settings`
  (`using_at_par_coupons`, default `true`) beside the evaluation date and the
  D11 fixing store. The behaviour it governs is fully reproduced: `index_fixing`
  computes the cached `fixingValueDate`/`fixingEndDate`/`spanningTime`
  (`couponpricer.cpp:56-88`) and reads the specialized 3-arg `forecastFixing`
  off the curve; a *determined* fixing is mode-independent and returns the same
  required past fixing as the C++ override. `BlackIborCouponPricer` ports
  `swapletRate = gearing * indexFixing + spread`;
  since a non-in-arrears `Black76` coupon needs no convexity adjustment it needs
  no volatility, and since only `swapletPrice` (not `swapletRate`) reads the
  forwarding-curve discount, the pricer captures no curve. The caplet/floorlet
  optionlet path, the `*Price` methods, and the in-arrears convexity adjustment
  belong to the cap/floor slice; an in-arrears coupon is refused with an `Err`
  rather than priced with a missing convexity term.

- **`IborLeg` builds the plain-vanilla coupons only; caps, floors and the
  in-arrears feature are omitted, not silently dropped.** QuantLib's `IborLeg`
  carries `withCaps`/`withFloors`/`inArrears` builder methods that route the leg
  through `CappedFlooredIborCoupon` and the optionlet pricer. Those belong to the
  cap/floor slice, so this port omits the methods entirely rather than accepting
  the vectors and building plain coupons. The default `BlackIborCouponPricer` is
  attached in `coupons()` (C++ attaches it in `operator Leg()` under the same
  no-cap/no-floor/no-arrears condition, which here always holds); `build()` only
  erases the concrete coupons into a `Leg`, and the free `set_coupon_pricer`
  overrides the default on the concrete coupons, the erased `Leg` carrying no
  downcast. The stub reference dates follow the `FloatingLeg` template
  (`calendar.adjust(end - tenor, bdc)`, no end-of-month flag), which agrees with
  the fixed leg on every multi-coupon schedule. A zero gearing, which the C++
  template collapses to a `FixedRateCoupon`, is not special-cased: `IborCoupon`
  rejects it, so `with_gearing(0.0)` surfaces that `Err` rather than a silent
  fixed coupon.
