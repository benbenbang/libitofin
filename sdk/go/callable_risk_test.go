package itofin

import (
	"encoding/csv"
	"errors"
	"math"
	"os"
	"strconv"
	"sync"
	"testing"
)

func callableRiskFixture(t *testing.T, s *Session, kind, right string) (*Bond, *TreeCallableFixedRateBondEngine, *DayCounter, *Settings) {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	reference := pricingMust(NewDate(12, 10, 2026))
	issue := pricingMust(NewDate(7, 7, 2026))
	maturity := pricingMust(NewDate(7, 7, 2031))
	bondCheck(t, settings.SetEvaluationDate(reference))
	dc := pricingMust(s.Actual365Fixed())
	cal := pricingMust(s.NullCalendar())
	curve := pricingMust(s.NewFlatForward(reference, .03, dc))
	model := pricingMust(s.NewHullWhite(curve, .1, .01))
	engine := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, 100, settings))
	var calls CallabilitySchedule
	if right != "none" {
		direction := CallabilityCall
		if right == "put" {
			direction = CallabilityPut
		}
		calls = CallabilitySchedule{pricingMust(NewCallability(100, Clean, direction, pricingMust(NewDate(7, 7, 2028))))}
	}
	convention := Unadjusted
	common := BondConfig{FaceAmount: 1000, Settings: settings, IssueDate: &issue, PaymentConvention: &convention}
	var bond *Bond
	if kind == "fixed" {
		schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: issue, End: maturity, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted}))
		product := pricingMust(s.NewCallableFixedRateBond(CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: common, Schedule: schedule, Coupons: []float64{.05}, AccrualDayCounter: dc}, Callability: calls}))
		bond = &product.Bond
	} else {
		common.Calendar = cal
		product := pricingMust(s.NewCallableZeroCouponBond(CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: common, MaturityDate: maturity}, DayCounter: dc, Callability: calls}))
		bond = &product.Bond
	}
	bondCheck(t, model.Close())
	bondCheck(t, curve.Close())
	return bond, engine, dc, settings
}

func TestCallableRiskQuantLib143NativeCleanNormalization(t *testing.T) {
	file := pricingMust(os.Open("testdata/callable_risk_native.tsv"))
	defer file.Close()
	reader := csv.NewReader(file)
	reader.Comma = '\t'
	rows := pricingMust(reader.ReadAll())
	if len(rows) != 37 {
		t.Fatal("expected 36 native oracle rows")
	}
	for _, row := range rows[1:] {
		t.Run(row[0]+row[1]+row[2]+row[3], func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			b, engine, dc, _ := callableRiskFixture(t, s, row[0], row[1])
			comp, freq := Continuous, Annual
			if row[2] == "compounded" {
				comp, freq = Compounded, Semiannual
			}
			if row[2] == "simple" {
				comp = Simple
			}
			bump := pricingMust(strconv.ParseFloat(row[3], 64))
			options := &CallableBondRiskOptions{Bump: &bump}
			for _, probe := range []struct {
				convexity bool
				index     int
				tolerance float64
			}{{false, 8, 2e-9}, {true, 9, 2e-6}} {
				expected := pricingMust(strconv.ParseFloat(row[probe.index], 64))
				got := pricingMust(callableRisk(b, .0075, engine, dc, comp, freq, options, probe.convexity))
				callableTreeNear(t, got, expected, probe.tolerance)
				if bump == 2e-4 {
					if absent := pricingMust(callableRisk(b, .0075, engine, dc, comp, freq, nil, probe.convexity)); absent != got {
						t.Fatal("native None bump changed")
					}
				}
			}
			if pricingMust(b.IsCalculated()) {
				t.Fatal("pure risk populated installed cache")
			}
			if _, err := b.NPV(); err == nil {
				t.Fatal("risk attached explicit engine")
			}
		})
	}
}

func TestCallableRiskTypedBoundariesSessionsAndRetention(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	b, e, dc, settings := callableRiskFixture(t, s, "zero", "call")
	zero := &CallableZeroCouponBond{*b}
	fixedBond, fixedEngine, fixedDC, _ := callableRiskFixture(t, s, "fixed", "call")
	fixed := &CallableFixedRateBond{*fixedBond}
	expected := pricingMust(zero.EffectiveDuration(.0075, e, dc, Continuous, Annual, nil))
	if math.IsNaN(pricingMust(fixed.EffectiveDuration(.0075, fixedEngine, fixedDC, Continuous, Annual, nil))) {
		t.Fatal("nonfinite fixed duration")
	}
	if math.IsNaN(pricingMust(fixed.EffectiveConvexity(.0075, fixedEngine, fixedDC, Continuous, Annual, nil))) {
		t.Fatal("nonfinite fixed convexity")
	}
	for _, bump := range []float64{0, -1, math.NaN(), math.Inf(1), 1e-320} {
		if _, err := zero.EffectiveDuration(.0075, e, dc, Continuous, Annual, &CallableBondRiskOptions{Bump: &bump}); err == nil {
			t.Fatal("invalid bump accepted")
		}
	}
	other := pricingMust(NewSession())
	defer other.Close()
	foreign := pricingMust(other.Actual365Fixed())
	if _, err := zero.EffectiveDuration(.0075, e, foreign, Continuous, Annual, nil); err == nil {
		t.Fatal("foreign session accepted")
	}
	wrong := pricingMust(s.NewSettings())
	bondCheck(t, wrong.SetEvaluationDate(pricingMust(NewDate(12, 10, 2026))))
	curve := pricingMust(s.NewFlatForward(pricingMust(NewDate(12, 10, 2026)), .03, dc))
	model := pricingMust(s.NewHullWhite(curve, .1, .01))
	wrongEngine := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, 100, wrong))
	if _, err := zero.EffectiveDuration(.0075, wrongEngine, dc, Continuous, Annual, nil); err == nil {
		t.Fatal("different Settings accepted")
	}
	bondCheck(t, settings.Close())
	if got := pricingMust(zero.EffectiveDuration(.0075, e, dc, Continuous, Annual, nil)); got != expected {
		t.Fatal("retained Settings changed")
	}
	var nilFixed *CallableFixedRateBond
	var nilZero *CallableZeroCouponBond
	if _, err := nilFixed.EffectiveDuration(0, e, dc, Continuous, Annual, nil); err == nil {
		t.Fatal("nil accepted")
	}
	if _, err := nilZero.EffectiveConvexity(0, e, dc, Continuous, Annual, nil); err == nil {
		t.Fatal("nil accepted")
	}
	if _, err := zero.EffectiveConvexity(0, nil, dc, Continuous, Annual, nil); err == nil {
		t.Fatal("nil engine accepted")
	}
	bondCheck(t, e.Close())
	if _, err := zero.EffectiveDuration(0, e, dc, Continuous, Annual, nil); err == nil {
		t.Fatal("released engine accepted")
	}
}

func TestCallableRiskConcurrentCallers(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	b, e, dc, _ := callableRiskFixture(t, s, "zero", "call")
	zero := &CallableZeroCouponBond{*b}
	expected := pricingMust(zero.EffectiveDuration(.0075, e, dc, Continuous, Annual, nil))
	failures := make(chan error, 8)
	var workers sync.WaitGroup
	for range 8 {
		workers.Go(func() {
			got, err := zero.EffectiveDuration(.0075, e, dc, Continuous, Annual, nil)
			if err != nil {
				failures <- err
				return
			}
			if got != expected {
				failures <- errors.New("concurrent risk result changed")
			}
		})
	}
	workers.Wait()
	close(failures)
	for err := range failures {
		t.Error(err)
	}
}
