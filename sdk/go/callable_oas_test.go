package itofin

import (
	"encoding/json"
	"math"
	"os"
	"sync"
	"testing"
)

type callableOASRow struct {
	Kind        string  `json:"kind"`
	Lag         uint32  `json:"lag"`
	Compounding string  `json:"compounding"`
	DayCounter  int     `json:"day_counter"`
	Spread      float64 `json:"spread"`
	Price       float64 `json:"normalized_clean"`
	Settlement  int32   `json:"settlement"`
}

func callableOASSample(t *testing.T, s *Session, kind string, lag uint32) (*Bond, *TreeCallableFixedRateBondEngine, *DayCounter, *Settings) {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	reference := pricingMust(NewDate(7, 7, 2026))
	maturity := pricingMust(NewDate(7, 7, 2031))
	bondCheck(t, settings.SetEvaluationDate(reference))
	dc := pricingMust(s.Actual365Fixed())
	dates := []Date{reference, pricingMust(NewDate(7, 7, 2027)), pricingMust(NewDate(7, 7, 2029)), pricingMust(NewDate(7, 7, 2036))}
	curve := pricingMust(s.NewDiscountCurve(NodeCurveConfig{Dates: dates, Values: []float64{1, 0.983, 0.915, 0.67}, DayCounter: dc}))
	model := pricingMust(s.NewHullWhite(curve, 0.1, 0.01))
	tree := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, 100, settings))
	rights := CallabilitySchedule{{Price: 100, Type: CallabilityCall, PriceType: Clean, Date: pricingMust(NewDate(7, 8, 2028))}, {Price: 97, Type: CallabilityPut, PriceType: Dirty, Date: pricingMust(NewDate(7, 1, 2029))}}
	cal := pricingMust(s.NullCalendar())
	conv := Unadjusted
	common := BondConfig{SettlementDays: lag, FaceAmount: 12345, Settings: settings, IssueDate: &reference, PaymentConvention: &conv}
	var b *Bond
	if kind == "fixed" {
		schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: reference, End: maturity, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted}))
		fixed := pricingMust(s.NewCallableFixedRateBond(CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: common, Schedule: schedule, Coupons: []float64{0.05}, AccrualDayCounter: dc}, Callability: rights}))
		b = &fixed.Bond
		bondCheck(t, schedule.Close())
	} else {
		common.Calendar = cal
		zero := pricingMust(s.NewCallableZeroCouponBond(CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: common, MaturityDate: maturity}, DayCounter: dc, Callability: rights}))
		b = &zero.Bond
	}
	bondCheck(t, model.Close())
	bondCheck(t, curve.Close())
	bondCheck(t, cal.Close())
	return b, tree, dc, settings
}
func TestCallableOASQuantLibOracles(t *testing.T) {
	var data struct {
		Version string           `json:"quantlib_version"`
		Cases   []callableOASRow `json:"cases"`
	}
	bondCheck(t, json.Unmarshal(pricingMust(os.ReadFile("testdata/callable_oas_oracle.json")), &data))
	if data.Version != "1.43" || len(data.Cases) != 30 {
		t.Fatal("independent native coverage")
	}
	conventions := map[string]Compounding{"Continuous": Continuous, "Compounded": Compounded, "Simple": Simple, "SimpleThenCompounded": SimpleThenCompounded, "CompoundedThenSimple": CompoundedThenSimple}
	for _, row := range data.Cases {
		s := pricingMust(NewSession())
		b, tree, dc, _ := callableOASSample(t, s, row.Kind, row.Lag)
		if row.DayCounter == 360 {
			dc = pricingMust(s.Actual360())
		}
		config := CallableOASConfig{DayCounter: dc, Compounding: conventions[row.Compounding], Frequency: Semiannual}
		price := pricingMust(callableOAS(b, row.Spread, tree, config, nil))
		callableTreeNear(t, price, row.Price, 2e-10)
		root := pricingMust(callableOAS(b, row.Price, tree, config, &AnalyticsSolverConfig{}))
		callableTreeNear(t, root, row.Spread, 2e-10)
		actual := pricingMust(DateFromSerial(row.Settlement))
		config.SettlementDate = &actual
		callableTreeNear(t, pricingMust(callableOAS(b, row.Spread, tree, config, nil)), row.Price, 2e-10)
		if pricingMust(b.IsCalculated()) {
			t.Fatal("pure probe populated cache")
		}
		bondCheck(t, s.Close())
	}
}
func TestCallableOASContractsAndSessions(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	for _, kind := range []string{"fixed", "zero"} {
		b, tree, dc, settings := callableOASSample(t, s, kind, 2)
		config := CallableOASConfig{DayCounter: dc, Compounding: Continuous, Frequency: Semiannual}
		fixed := &CallableFixedRateBond{*b}
		zero := &CallableZeroCouponBond{*b}
		price := pricingMust(callableOAS(b, 0.015, tree, config, nil))
		if kind == "fixed" {
			callableTreeNear(t, pricingMust(fixed.CleanPriceOAS(0.015, tree, config)), price, 0)
			callableTreeNear(t, pricingMust(fixed.OAS(price, tree, CallableOASSolveConfig{CallableOASConfig: config})), 0.015, 2e-10)
			bondCheck(t, fixed.SetTreeEngine(tree))
		} else {
			callableTreeNear(t, pricingMust(zero.CleanPriceOAS(0.015, tree, config)), price, 0)
			callableTreeNear(t, pricingMust(zero.OAS(price, tree, CallableOASSolveConfig{CallableOASConfig: config})), 0.015, 2e-10)
			bondCheck(t, zero.SetTreeEngine(tree))
		}
		old := pricingMust(b.NPV())
		bondCheck(t, b.Freeze())
		one, none, bad := uint(1), uint(0), 0.0
		for _, solve := range []AnalyticsSolverConfig{{MaxEvaluations: &one}, {MaxEvaluations: &none}, {Accuracy: &bad}} {
			if _, err := callableOAS(b, price, tree, config, &solve); err == nil {
				t.Fatal("invalid solver accepted")
			}
		}
		for _, badValue := range []float64{math.NaN(), math.Inf(1)} {
			if _, err := callableOAS(b, badValue, tree, config, nil); err == nil {
				t.Fatal("nonfinite accepted")
			}
		}
		if pricingMust(b.NPV()) != old || !pricingMust(b.IsCalculated()) {
			t.Fatal("frozen cache changed")
		}
		bondCheck(t, settings.Close())
		var wg sync.WaitGroup
		for i := 0; i < 4; i++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				if _, err := callableOAS(b, 0.015, tree, config, nil); err != nil {
					t.Error(err)
				}
			}()
		}
		wg.Wait()
		other := pricingMust(NewSession())
		foreign := pricingMust(other.Actual365Fixed())
		cross := config
		cross.DayCounter = foreign
		if _, err := callableOAS(b, 0.015, tree, cross, nil); err == nil {
			t.Fatal("cross-session accepted")
		}
		bondCheck(t, other.Close())
		_, wrong, _, _ := callableOASSample(t, s, kind, 2)
		if _, err := callableOAS(b, 0.015, wrong, config, nil); err == nil {
			t.Fatal("distinct settings accepted")
		}
		invalid := config
		invalid.SettlementDate = &Date{}
		if _, err := callableOAS(b, 0.015, tree, invalid, nil); err == nil {
			t.Fatal("explicit null settlement accepted")
		}
		if _, err := callableOAS(b, 0.015, nil, config, nil); err == nil {
			t.Fatal("nil engine accepted")
		}
		bondCheck(t, tree.Close())
		if _, err := callableOAS(b, 0.015, tree, config, nil); err == nil {
			t.Fatal("released engine accepted")
		}
	}
	var absent *CallableFixedRateBond
	if _, err := absent.CleanPriceOAS(0, nil, CallableOASConfig{}); err == nil {
		t.Fatal("nil bond accepted")
	}
}
