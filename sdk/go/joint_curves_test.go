package itofin

import (
	"errors"
	"math"
	"testing"
)

func TestJointYieldCurvesQuantLibRepricingAndUpdates(t *testing.T) {
	m := newJointMarket(t)
	_, curves := m.curves(t)
	for _, helper := range m.basisHelpers {
		jointNear(t, sessionMust(helper.QuoteValue()), .002, 0)
		if _, err := helper.ImpliedQuote(); err == nil {
			t.Fatal("joint assembly rebound the standalone basis template")
		}
	}
	m.reprice(t, curves)
	for _, update := range []func(){
		func() { pricingOK(t, m.basis.SetValue(.0025)) },
		func() { pricingOK(t, m.rate.SetValue(.031)) },
		func() { pricingOK(t, m.discountQuote.SetValue(.025)) },
		func() { m.today = curveDate(t, 24, 10, 2025); pricingOK(t, m.settings.SetEvaluationDate(m.today)) },
	} {
		before := sessionMust(curves[0].Discount(5, false))
		update()
		after := sessionMust(curves[0].Discount(5, false))
		jointFinite(t, before, after)
		if math.Abs(after-before) < 1e-10 {
			t.Fatal("market update did not invalidate warmed joint curve")
		}
		m.reprice(t, curves)
		fresh, freshCurves := m.curves(t)
		for i := range curves {
			jointNear(t, sessionMust(curves[i].Discount(5, false)), sessionMust(freshCurves[i].Discount(5, false)), 1e-10)
			pricingOK(t, freshCurves[i].Close())
		}
		pricingOK(t, fresh.Close())
	}
}

func TestJointYieldCurvesFixingsAndCloseOrders(t *testing.T) {
	for _, base := range []bool{true, false} {
		for _, historical := range []bool{true, false} {
			t.Logf("base=%v historical=%v", base, historical)
			m := newJointMarket(t)
			m.omitSixMonth = true
			if historical {
				m.today = curveDate(t, 24, 10, 2025)
				pricingOK(t, m.settings.SetEvaluationDate(m.today))
				m.basisSettlementDays = 0
			}
			for _, helper := range m.basisHelpers {
				pricingOK(t, helper.Close())
			}
			m.basisHelpers = nil
			for i := int32(2); i <= 10; i++ {
				m.basisHelpers = append(m.basisHelpers, sessionMust(m.s.NewIborIborBasisSwapRateHelper(m.basisConfig(Period{i, Years}, true))))
			}
			for _, months := range []int32{12, 18} {
				m.basisHelpers = append(m.basisHelpers, sessionMust(m.s.NewIborIborBasisSwapRateHelper(m.basisConfig(Period{months, Months}, false))))
			}
			owner, curves := m.curves(t)
			var before [2]float64
			if historical {
				known := m.base
				if base {
					known = m.other
				}
				pricingOK(t, known.AddFixing(curveDate(t, 22, 10, 2025), .03))
				if _, err := curves[0].Discount(5, false); err == nil {
					t.Fatal("missing historical fixing was accepted")
				}
			} else {
				for i := range curves {
					before[i] = sessionMust(curves[i].Discount(5, false))
				}
			}
			index := m.other
			if base {
				index = m.base
			}
			fixingDate := m.today
			if historical {
				fixingDate = curveDate(t, 22, 10, 2025)
			}
			pricingOK(t, index.AddFixing(fixingDate, .035))
			after := [2]float64{sessionMust(curves[0].Discount(5, false)), sessionMust(curves[1].Discount(5, false))}
			jointFinite(t, before[0], before[1], after[0], after[1])
			if !historical && math.Max(math.Abs(before[0]-after[0]), math.Abs(before[1]-after[1])) < 1e-7 {
				t.Fatalf("fixing did not reach kept/cloned indices: base=%v historical=%v", base, historical)
			}
			m.reprice(t, curves)
			fresh, freshCurves := m.curves(t)
			m.reprice(t, freshCurves)
			for i := range curves {
				jointNear(t, sessionMust(curves[i].Discount(5, false)), sessionMust(freshCurves[i].Discount(5, false)), 1e-10)
				pricingOK(t, freshCurves[i].Close())
			}
			pricingOK(t, fresh.Close())
			retained := sessionMust(m.s.NewEuribor(Period{3, Months}, curves[0], m.settings))
			if base {
				pricingOK(t, owner.Close())
			}
			for _, curve := range curves {
				pricingOK(t, curve.Close())
			}
			if !base {
				pricingOK(t, owner.Close())
			}
			pricingOK(t, m.base.Close())
			pricingOK(t, m.other.Close())
			for _, helper := range m.basisHelpers {
				pricingOK(t, helper.Close())
			}
			pricingOK(t, m.discount.Close())
			pricingOK(t, m.basis.SetValue(.0027))
			pricingOK(t, m.rate.Close())
			pricingOK(t, m.basis.Close())
			pricingOK(t, m.discountQuote.Close())
			future := sessionMust(m.cal.Advance(m.today, 5, Years, Following, false))
			value := sessionMust(retained.Fixing(future, true))
			if math.IsNaN(value) || math.IsInf(value, 0) {
				t.Fatal("retained joint curve did not rebootstrap")
			}
			pricingOK(t, retained.Close())
			pricingOK(t, m.s.Close())
		}
	}
}

func TestJointYieldCurvesInvalidInputs(t *testing.T) {
	m := newJointMarket(t)
	other := newJointMarket(t)
	base := m.basisConfig(Period{2, Years}, true)
	for _, change := range []func(*BasisHelperConfig){
		func(c *BasisHelperConfig) { c.Quote = nil },
		func(c *BasisHelperConfig) { c.Calendar = nil },
		func(c *BasisHelperConfig) { c.BaseIndex = nil },
		func(c *BasisHelperConfig) { c.OtherIndex = nil },
		func(c *BasisHelperConfig) { c.DiscountCurve = nil },
		func(c *BasisHelperConfig) { c.Tenor.Length = 0 },
		func(c *BasisHelperConfig) { c.Tenor.Unit = TimeUnit(99) },
		func(c *BasisHelperConfig) { c.Convention = BusinessDayConvention(99) },
		func(c *BasisHelperConfig) { c.SettlementDays = ^uint32(0) },
		func(c *BasisHelperConfig) { c.Quote = other.basis },
		func(c *BasisHelperConfig) { c.Calendar = other.cal },
		func(c *BasisHelperConfig) { c.BaseIndex = other.base },
		func(c *BasisHelperConfig) { c.OtherIndex = other.other },
		func(c *BasisHelperConfig) { c.DiscountCurve = other.discount },
	} {
		cfg := base
		change(&cfg)
		if _, err := m.s.NewIborIborBasisSwapRateHelper(cfg); err == nil {
			t.Fatal("invalid basis helper accepted")
		}
	}
	for _, change := range []func(*JointYieldCurvesConfig){
		func(c *JointYieldCurvesConfig) { c.DayCounter = nil },
		func(c *JointYieldCurvesConfig) { c.DayCounter = other.dc },
		func(c *JointYieldCurvesConfig) { c.ReferenceDate = Date{} },
		func(c *JointYieldCurvesConfig) { c.Accuracy = 0 },
		func(c *JointYieldCurvesConfig) { c.Accuracy = math.NaN() },
		func(c *JointYieldCurvesConfig) { c.BasisHelpers = nil },
		func(c *JointYieldCurvesConfig) { c.BasisHelpers = c.BasisHelpers[:9] },
		func(c *JointYieldCurvesConfig) { c.FirstHelpers[0] = nil },
		func(c *JointYieldCurvesConfig) { c.SecondHelpers[0] = nil },
		func(c *JointYieldCurvesConfig) { c.BasisHelpers = []*RateHelper{nil} },
		func(c *JointYieldCurvesConfig) { c.FirstHelpers[0] = other.basisHelpers[0] },
		func(c *JointYieldCurvesConfig) { c.SecondHelpers[0] = other.basisHelpers[0] },
		func(c *JointYieldCurvesConfig) { c.BasisHelpers = other.basisHelpers },
		func(c *JointYieldCurvesConfig) { c.BasisHelpers = c.FirstHelpers },
		func(c *JointYieldCurvesConfig) { c.SecondHelpers = c.FirstHelpers },
	} {
		cfg := m.config()
		change(&cfg)
		if _, err := m.s.NewJointYieldCurves(cfg); err == nil {
			t.Fatal("invalid joint configuration accepted")
		}
	}
	joint, curves := m.curves(t)
	for _, member := range []int{-1, 2, int(^uint(0) >> 1)} {
		if _, err := joint.Curve(member); err == nil {
			t.Fatal("invalid member accepted")
		}
	}
	pricingOK(t, joint.Close())
	if _, err := joint.Curve(0); err == nil {
		t.Fatal("closed joint owner accepted")
	}
	for _, curve := range curves {
		pricingOK(t, curve.Close())
	}
	pricingOK(t, m.base.Close())
	if _, err := m.s.NewIborIborBasisSwapRateHelper(base); err == nil {
		t.Fatal("closed helper dependency accepted")
	}
	if err := m.base.AddFixing(m.today, .04); err == nil {
		t.Fatal("closed index accepted")
	}
	if _, err := other.s.NewFlatForwardFromQuote(m.today, m.basis, other.dc); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign quote: %v", err)
	}
	if _, err := m.s.NewFlatForwardFromQuote(m.today, nil, m.dc); err == nil {
		t.Fatal("nil quote accepted")
	}
	if _, err := m.s.NewSwapRateHelperWithDiscount(SwapRateHelperConfig{}, m.discount); err == nil {
		t.Fatal("nil swap inputs accepted")
	}
	var empty *JointYieldCurves
	if _, err := empty.Curve(0); err == nil {
		t.Fatal("nil joint accepted")
	}
}

func TestStandaloneBasisHelperBothBootstrapSides(t *testing.T) {
	for _, side := range []bool{true, false} {
		m := newJointMarket(t)
		base := sessionMust(m.s.NewEuribor(Period{3, Months}, m.discount, m.settings))
		other := sessionMust(m.s.NewEuribor(Period{6, Months}, m.discount, m.settings))
		cfg := m.basisConfig(Period{2, Years}, side)
		cfg.BaseIndex, cfg.OtherIndex = base, other
		helper := sessionMust(m.s.NewIborIborBasisSwapRateHelper(cfg))
		curve := sessionMust(m.s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{ReferenceDate: m.today, Helpers: []*RateHelper{helper}, DayCounter: m.dc}))
		for _, spread := range []float64{.002, .0025} {
			pricingOK(t, m.basis.SetValue(spread))
			jointFinite(t, sessionMust(curve.Discount(1, false)))
			jointNear(t, sessionMust(helper.ImpliedQuote()), spread, 1e-10)
			first, second := m.discount, curve
			if side {
				first, second = curve, m.discount
			}
			fittedBase := sessionMust(m.s.NewEuribor(Period{3, Months}, first, m.settings))
			fittedOther := sessionMust(m.s.NewEuribor(Period{6, Months}, second, m.settings))
			spot := sessionMust(m.cal.Advance(m.today, 2, Days, Following, false))
			end := sessionMust(m.cal.Advance(spot, 2, Years, ModifiedFollowing, false))
			npv := m.legNPV(t, fittedBase, spot, end, Quarterly, spread) - m.legNPV(t, fittedOther, spot, end, Semiannual, 0)
			jointNear(t, npv, 0, 1e-10)
			pricingOK(t, fittedBase.Close())
			pricingOK(t, fittedOther.Close())
		}
	}
}
