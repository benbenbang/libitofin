package itofin

import (
	"math"
	"testing"
)

func TestYoYInflationCouponsAndCapFloorFactories(t *testing.T) {
	s, settings, index, cal, dc, discount := inflProductsQuotedMarket(t)
	rule := Backward
	schedule, err := s.NewSchedule(ScheduleConfig{Start: testDate(t, 13, 8, 2007), End: testDate(t, 13, 8, 2012), Frequency: Annual, Calendar: cal, Convention: Unadjusted, Rule: &rule})
	inflProductsCheck(t, err)
	notional := 1e6
	cfg := YoYInflationLegConfig{Schedule: schedule, PaymentCalendar: cal, Index: index, ObservationLag: Period{2, Months}, Interpolation: CpiFlat, PaymentDayCounter: dc, Notional: &notional}
	leg, err := s.NewYoYInflationLeg(cfg)
	inflProductsCheck(t, err)
	coupons, err := leg.Coupons()
	inflProductsCheck(t, err)
	if len(coupons) != 5 {
		t.Fatal(len(coupons))
	}
	coupon := coupons[0]
	rate, err := coupon.Rate()
	inflProductsCheck(t, err)
	inflProductsNear(t, rate, .03, 1e-12)
	amount, err := coupon.Amount()
	inflProductsCheck(t, err)
	inflProductsNear(t, amount, 30000, 1e-7)
	fixing, err := coupon.IndexFixing()
	inflProductsCheck(t, err)
	inflProductsNear(t, fixing, .03, 1e-12)
	for _, check := range []struct {
		read func() (float64, error)
		want float64
	}{{coupon.Nominal, 1e6}, {coupon.AccrualPeriod, 1}, {coupon.Gearing, 1}, {coupon.Spread, 0}} {
		v, err := check.read()
		inflProductsCheck(t, err)
		inflProductsNear(t, v, check.want, 1e-12)
	}
	for _, read := range []func() (Date, error){coupon.FixingDate, coupon.AccrualStartDate, coupon.AccrualEndDate, coupon.Date} {
		d, err := read()
		inflProductsCheck(t, err)
		if d.Serial() == 0 {
			t.Fatal(d)
		}
	}
	readDC, err := coupon.DayCounter()
	inflProductsCheck(t, err)
	equal, err := readDC.Equal(dc)
	inflProductsCheck(t, err)
	if !equal {
		t.Fatal("wrong day count")
	}
	lag, err := coupon.ObservationLag()
	inflProductsCheck(t, err)
	if !lag.Equal(cfg.ObservationLag) {
		t.Fatal(lag)
	}
	interp, err := coupon.Interpolation()
	inflProductsCheck(t, err)
	if interp != CpiFlat {
		t.Fatal(interp)
	}
	days, err := coupon.FixingDays()
	inflProductsCheck(t, err)
	if days != 0 {
		t.Fatal(days)
	}
	repr, err := coupon.Repr()
	inflProductsCheck(t, err)
	if repr == "" {
		t.Fatal("empty repr")
	}
	erased, err := leg.Build()
	inflProductsCheck(t, err)
	size, err := erased.Len()
	inflProductsCheck(t, err)
	if size != 5 {
		t.Fatal(size)
	}
	vol, err := s.NewConstantYoYOptionletVolatility(ConstantYoYOptionletVolatilityConfig{Volatility: .01, Calendar: cal, Convention: ModifiedFollowing, DayCounter: dc, ObservationLag: Period{2, Months}, Frequency: Monthly, MinStrike: -1, MaxStrike: 100, Settings: settings})
	inflProductsCheck(t, err)
	cap, err := s.NewYoYInflationCap(coupons, []float64{.03}, settings)
	inflProductsCheck(t, err)
	floor, err := s.NewYoYInflationFloor(coupons, []float64{.03}, settings)
	inflProductsCheck(t, err)
	collar, err := s.NewYoYInflationCollar(coupons, []float64{.03}, []float64{.03}, settings)
	inflProductsCheck(t, err)
	engine, err := s.NewBlackYoYInflationCapFloorEngine(index, vol, discount)
	inflProductsCheck(t, err)
	capPrice, err := cap.Price(engine)
	inflProductsCheck(t, err)
	floorPrice, err := floor.Price(engine)
	inflProductsCheck(t, err)
	collarPrice, err := collar.Price(engine)
	inflProductsCheck(t, err)
	inflProductsNear(t, collarPrice, capPrice-floorPrice, 1e-8)
	single, err := s.NewYoYInflationCapFloorWithStrikes(CapType, coupons, []float64{.03}, settings)
	inflProductsCheck(t, err)
	same, err := single.Price(engine)
	inflProductsCheck(t, err)
	inflProductsNear(t, same, capPrice, 1e-8)
	if _, err := s.NewYoYInflationCapFloorWithStrikes(CollarType, coupons, []float64{.03}, settings); err == nil {
		t.Fatal("single strike collar accepted")
	}
	if _, err := s.NewYoYInflationCap(nil, []float64{.03}, settings); err == nil {
		t.Fatal("empty coupons accepted")
	}
	cfg.Caps = []float64{.04}
	cfg.Floors = []float64{.02}
	cfg.Notionals = []float64{1e6}
	cfg.Gearings = []float64{1}
	cfg.Spreads = []float64{.001}
	cappedLeg, err := s.NewYoYInflationLeg(cfg)
	inflProductsCheck(t, err)
	for _, factory := range []func(*ConstantYoYOptionletVolatility, *YieldTermStructure) (*YoYInflationOptionletCouponPricer, error){s.NewBlackYoYInflationOptionletCouponPricer, s.NewUnitDisplacedYoYInflationOptionletCouponPricer, s.NewBachelierYoYInflationOptionletCouponPricer} {
		pricer, err := factory(vol, nil)
		inflProductsCheck(t, err)
		capped, err := cappedLeg.CappedFlooredCoupons(pricer)
		inflProductsCheck(t, err)
		if len(capped) != 5 {
			t.Fatal(len(capped))
		}
		c := capped[0]
		yes, err := c.IsCapped()
		inflProductsCheck(t, err)
		if !yes {
			t.Fatal("cap missing")
		}
		yes, err = c.IsFloored()
		inflProductsCheck(t, err)
		if !yes {
			t.Fatal("floor missing")
		}
		effective, err := c.EffectiveCap()
		inflProductsCheck(t, err)
		inflProductsNear(t, effective, .039, 1e-14)
		effective, err = c.EffectiveFloor()
		inflProductsCheck(t, err)
		inflProductsNear(t, effective, .019, 1e-14)
		rate, err := c.Rate()
		inflProductsCheck(t, err)
		amount, err := c.Amount()
		inflProductsCheck(t, err)
		if math.IsNaN(rate) {
			t.Fatal(rate)
		}
		inflProductsNear(t, amount, rate*1e6, 1e-8)
		repr, err := c.Repr()
		inflProductsCheck(t, err)
		if repr == "" {
			t.Fatal("empty repr")
		}
		inflProductsCheck(t, pricer.Close())
		if _, err = c.Rate(); err != nil {
			t.Fatal("pricer lifetime", err)
		}
	}
	bad := cfg
	bad.FixingDays = math.MaxUint32
	if _, err = s.NewYoYInflationLeg(bad); err == nil {
		t.Fatal("wrapped fixing days accepted")
	}
	inflProductsCheck(t, leg.Close())
	if _, err = coupon.Rate(); err != nil {
		t.Fatal("coupon lifetime", err)
	}
	inflProductsCheck(t, coupon.Close())
	if _, err = coupon.Rate(); err == nil {
		t.Fatal("released coupon usable")
	}
}
