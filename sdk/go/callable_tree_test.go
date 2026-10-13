package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"sync"
	"testing"
)

type callableTreeSnapshot struct {
	NPV                float64 `json:"npv"`
	SettlementValue    float64 `json:"settlement_value"`
	Clean              float64 `json:"clean"`
	Dirty              float64 `json:"dirty"`
	Accrued            float64 `json:"accrued"`
	Settlement         int32   `json:"settlement"`
	RedemptionDate     int32   `json:"redemption_date"`
	SettlementDiscount float64 `json:"settlement_discount"`
}
type callableTreeCase struct {
	Name              string                `json:"name"`
	Kind              string                `json:"kind"`
	Reference         int32                 `json:"reference"`
	Evaluation        int32                 `json:"evaluation"`
	Start             int32                 `json:"start"`
	Maturity          int32                 `json:"maturity"`
	Issue             int32                 `json:"issue"`
	Calendar          string                `json:"calendar"`
	SettlementDays    uint32                `json:"settlement_days"`
	Face              float64               `json:"face"`
	Redemption        float64               `json:"redemption"`
	PaymentConvention BusinessDayConvention `json:"payment_convention"`
	Coupons           []float64             `json:"coupons"`
	ExLength          int32                 `json:"ex_length"`
	ExUnit            TimeUnit              `json:"ex_unit"`
	ExEOM             bool                  `json:"ex_eom"`
	ScheduleRule      DateGeneration        `json:"schedule_rule"`
	Rate              float64               `json:"rate"`
	A                 float64               `json:"a"`
	Sigma             float64               `json:"sigma"`
	Steps             uint                  `json:"steps"`
	Events            [][]float64           `json:"events"`
	CurveDates        []int32               `json:"curve_dates"`
	CurveRates        []float64             `json:"curve_rates"`
	CurrencyTolerance float64               `json:"currency_tolerance"`
	PriceTolerance    float64               `json:"price_tolerance"`
	Expected          callableTreeSnapshot  `json:"expected"`
	UpdateRate        *float64              `json:"update_rate"`
	UpdatedRate       *callableTreeSnapshot `json:"updated_rate"`
	UpdatedEvaluation *callableTreeSnapshot `json:"updated_evaluation"`
}

func callableTreeRows(t *testing.T) []callableTreeCase {
	t.Helper()
	data := pricingMust(os.ReadFile("testdata/callable_tree_oracle.json"))
	var oracle struct {
		Version string             `json:"quantlib_version"`
		Cases   []callableTreeCase `json:"cases"`
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	if oracle.Version != "1.43" || len(oracle.Cases) != 36 {
		t.Fatal("incomplete independent tree oracle")
	}
	return oracle.Cases
}
func callableTreeNear(t *testing.T, got, want, tolerance float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > tolerance {
		t.Fatalf("got %.17g want %.17g tolerance %.17g", got, want, tolerance)
	}
}
func callableTreeFixture(t *testing.T, s *Session, row callableTreeCase) (*Bond, func(*TreeCallableFixedRateBondEngine) error, *TreeCallableFixedRateBondEngine, *HullWhite, *YieldTermStructure, *SimpleQuote, *Settings, *DayCounter) {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(pricingMust(DateFromSerial(row.Evaluation))))
	cal := pricingMust(s.NullCalendar())
	if row.Calendar == "target" {
		bondCheck(t, cal.Close())
		cal = pricingMust(s.Target())
	}
	dc := pricingMust(s.Actual365Fixed())
	quote := pricingMust(s.NewSimpleQuote(row.Rate))
	var curve *YieldTermStructure
	if len(row.CurveDates) == 0 {
		curve = pricingMust(s.NewFlatForwardFromQuote(pricingMust(DateFromSerial(row.Reference)), quote, dc))
	} else {
		dates := make([]Date, len(row.CurveDates))
		for i, serial := range row.CurveDates {
			dates[i] = pricingMust(DateFromSerial(serial))
		}
		curve = pricingMust(s.NewZeroCurve(NodeCurveConfig{Dates: dates, Values: row.CurveRates, DayCounter: dc}))
	}
	model := pricingMust(s.NewHullWhite(curve, row.A, row.Sigma))
	engine := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, row.Steps, settings))
	calls := make(CallabilitySchedule, len(row.Events))
	for i, event := range row.Events {
		calls[i] = pricingMust(NewCallability(event[0], BondPriceType(event[1]), CallabilityType(event[2]), pricingMust(DateFromSerial(int32(event[3])))))
	}
	issue := pricingMust(DateFromSerial(row.Issue))
	config := BondConfig{SettlementDays: row.SettlementDays, FaceAmount: row.Face, Redemption: &row.Redemption, Settings: settings, IssueDate: &issue, PaymentConvention: &row.PaymentConvention}
	var bond *Bond
	var set func(*TreeCallableFixedRateBondEngine) error
	if row.Kind == "zero" {
		config.Calendar = cal
		zero := pricingMust(s.NewCallableZeroCouponBond(CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: config, MaturityDate: pricingMust(DateFromSerial(row.Maturity))}, DayCounter: dc, Callability: calls}))
		bond, set = &zero.Bond, zero.SetTreeEngine
	} else {
		schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: pricingMust(DateFromSerial(row.Start)), End: pricingMust(DateFromSerial(row.Maturity)), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &row.ScheduleRule}))
		cfg := CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: config, Schedule: schedule, Coupons: row.Coupons, AccrualDayCounter: dc, ExCouponEndOfMonth: row.ExEOM}, Callability: calls}
		if row.ExLength != 0 {
			cfg.ExCouponPeriod = &Period{row.ExLength, row.ExUnit}
		}
		fixed := pricingMust(s.NewCallableFixedRateBond(cfg))
		if row.ScheduleRule == Zero {
			if _, err := fixed.Frequency(); err == nil {
				t.Fatal("native Once ordinal fabricated")
			}
		}
		bond, set = &fixed.Bond, fixed.SetTreeEngine
		bondCheck(t, schedule.Close())
	}
	bondCheck(t, set(engine))
	bondCheck(t, cal.Close())
	return bond, set, engine, model, curve, quote, settings, dc
}
func callableTreeAssert(t *testing.T, b *Bond, row callableTreeCase, want callableTreeSnapshot) {
	t.Helper()
	for _, probe := range []struct {
		read            func() (float64, error)
		want, tolerance float64
	}{
		{b.NPV, want.NPV, row.CurrencyTolerance}, {b.SettlementValue, want.SettlementValue, row.CurrencyTolerance}, {b.CleanPrice, want.Clean, row.PriceTolerance}, {b.DirtyPrice, want.Dirty, row.PriceTolerance}, {func() (float64, error) { return b.AccruedAmount(nil) }, want.Accrued, 1e-14},
	} {
		callableTreeNear(t, pricingMust(probe.read()), probe.want, probe.tolerance)
	}
	if pricingMust(b.SettlementDate(nil)).Serial() != want.Settlement {
		t.Fatal("tree settlement date")
	}
	redemptions := pricingMust(b.Redemptions())
	flow := pricingMust(redemptions.At(0))
	if pricingMust(flow.Date()).Serial() != want.RedemptionDate {
		t.Fatal("adjusted redemption date")
	}
	bondCheck(t, flow.Close())
	bondCheck(t, redemptions.Close())
	callableTreeNear(t, pricingMust(b.SettlementValue()), pricingMust(b.NPV())/want.SettlementDiscount, row.CurrencyTolerance)
	if !pricingMust(b.IsCalculated()) {
		t.Fatal("actual tree lazy cache not shared")
	}
}
func TestCallableTreeQuantLibOracles(t *testing.T) {
	for _, row := range callableTreeRows(t) {
		t.Run(row.Name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			b, _, engine, model, curve, quote, settings, _ := callableTreeFixture(t, s, row)
			callableTreeAssert(t, b, row, row.Expected)
			if row.Name == "zero_zero_redemption_put" && pricingMust(b.NPV()) <= 0 {
				t.Fatal("active positive put on zero redemption must have value")
			}
			if row.UpdateRate != nil {
				bondCheck(t, quote.SetValue(*row.UpdateRate))
				if pricingMust(b.IsCalculated()) {
					t.Fatal("live curve quote did not invalidate tree")
				}
				callableTreeAssert(t, b, row, *row.UpdatedRate)
				bondCheck(t, settings.SetEvaluationDate(pricingMust(DateFromSerial(row.Evaluation+1))))
				if pricingMust(b.IsCalculated()) {
					t.Fatal("evaluation update did not invalidate tree")
				}
				callableTreeAssert(t, b, row, *row.UpdatedEvaluation)
			}
			bondCheck(t, engine.Close())
			bondCheck(t, model.Close())
			bondCheck(t, curve.Close())
			bondCheck(t, b.Recalculate())
			want := row.Expected
			if row.UpdatedEvaluation != nil {
				want = *row.UpdatedEvaluation
			}
			callableTreeAssert(t, b, row, want)
		})
	}
}
func TestCallableTreeRetainedLifecycleAndReplacement(t *testing.T) {
	for _, kind := range []string{"fixed", "zero"} {
		t.Run(kind, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			row := callableTreeRows(t)[1]
			if kind == "zero" {
				row.Kind, row.Events, row.Coupons = "zero", [][]float64{{100, 0, 0, float64(testDate(t, 7, 7, 2028).Serial())}}, nil
			}
			b, set, oldEngine, oldModel, oldCurve, oldQuote, settings, dc := callableTreeFixture(t, s, row)
			initial := pricingMust(b.NPV())
			bondCheck(t, b.Freeze())
			bondCheck(t, oldQuote.SetValue(.045))
			if pricingMust(b.IsCalculated()) {
				t.Fatal("frozen update must invalidate flag")
			}
			if pricingMust(b.NPV()) != initial {
				t.Fatal("freeze did not pin native cached price")
			}
			bondCheck(t, b.Recalculate())
			frozenUpdated := pricingMust(b.NPV())
			if frozenUpdated == initial {
				t.Fatal("forced frozen recalculation did not use live curve")
			}
			bondCheck(t, oldQuote.SetValue(.05))
			if pricingMust(b.NPV()) != frozenUpdated {
				t.Fatal("recalculate lost freeze state")
			}
			bondCheck(t, b.Unfreeze())
			if pricingMust(b.NPV()) == frozenUpdated {
				t.Fatal("unfreeze did not resume pricing")
			}
			newQuote := pricingMust(s.NewSimpleQuote(.025))
			newCurve := pricingMust(s.NewFlatForwardFromQuote(pricingMust(DateFromSerial(row.Reference)), newQuote, dc))
			newModel := pricingMust(s.NewHullWhite(newCurve, row.A, row.Sigma))
			newEngine := pricingMust(s.NewTreeCallableFixedRateBondEngine(newModel, row.Steps, settings))
			bondCheck(t, set(newEngine))
			replacement := pricingMust(b.NPV())
			bondCheck(t, oldQuote.SetValue(.06))
			if !pricingMust(b.IsCalculated()) || pricingMust(b.NPV()) != replacement {
				t.Fatal("old tree graph still invalidates replacement")
			}
			bondCheck(t, newQuote.SetValue(.035))
			if pricingMust(b.IsCalculated()) {
				t.Fatal("replacement graph was not retained")
			}
			current := pricingMust(b.NPV())
			if current == replacement {
				t.Fatal("replacement quote ignored")
			}
			for _, close := range []func() error{oldEngine.Close, oldModel.Close, oldCurve.Close, newEngine.Close, newModel.Close, newCurve.Close, dc.Close} {
				bondCheck(t, close())
			}
			bondCheck(t, newQuote.SetValue(.04))
			if pricingMust(b.NPV()) == current {
				t.Fatal("closed dependency facades destroyed graph")
			}
			bondCheck(t, b.Freeze())
			bondCheck(t, settings.SetEvaluationDate(testDate(t, 8, 7, 2031)))
			callableTreeNear(t, pricingMust(b.NPV()), 0, 0)
			callableTreeNear(t, pricingMust(b.CleanPrice()), 0, 0)
			bondCheck(t, b.Close())
			if _, err := b.NPV(); err == nil {
				t.Fatal("closed callable priced")
			}
		})
	}
}
func TestCallableTreeValidationAndSessions(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	s2 := pricingMust(NewSession())
	defer s2.Close()
	row := callableTreeRows(t)[1]
	b, set, engine, model, curve, quote, settings, dc := callableTreeFixture(t, s, row)
	if _, err := s.NewTreeCallableFixedRateBondEngine(nil, 100, settings); err == nil {
		t.Fatal("nil model")
	}
	if _, err := s.NewTreeCallableFixedRateBondEngine(model, 100, nil); err == nil {
		t.Fatal("nil settings")
	}
	if _, err := s.NewTreeCallableFixedRateBondEngine(model, 0, settings); err == nil {
		t.Fatal("zero steps")
	}
	otherSettings := pricingMust(s.NewSettings())
	bondCheck(t, otherSettings.SetEvaluationDate(pricingMust(DateFromSerial(row.Evaluation))))
	wrong := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, 100, otherSettings))
	cached := pricingMust(b.NPV())
	if err := set(wrong); err == nil {
		t.Fatal("different allocation settings accepted")
	}
	if !pricingMust(b.IsCalculated()) || pricingMust(b.NPV()) != cached {
		t.Fatal("failed attachment changed cache")
	}
	if err := set(nil); err == nil {
		t.Fatal("nil tree engine")
	}
	foreignSettings := pricingMust(s2.NewSettings())
	if _, err := s.NewTreeCallableFixedRateBondEngine(model, 100, foreignSettings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("foreign settings", err)
	}
	foreignDC := pricingMust(s2.Actual365Fixed())
	foreignCurve := pricingMust(s2.NewFlatForward(pricingMust(DateFromSerial(row.Reference)), .03, foreignDC))
	foreignModel := pricingMust(s2.NewHullWhite(foreignCurve, row.A, row.Sigma))
	foreignEngine := pricingMust(s2.NewTreeCallableFixedRateBondEngine(foreignModel, 100, foreignSettings))
	if err := set(foreignEngine); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("foreign tree", err)
	}
	for _, rate := range []float64{math.NaN(), math.Inf(1), 1000} {
		bondCheck(t, quote.SetValue(rate))
		deferred := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, 100, settings))
		bondCheck(t, deferred.Close())
		if _, err := b.NPV(); err == nil {
			t.Fatal("invalid live curve valued")
		}
		bondCheck(t, quote.SetValue(row.Rate))
		callableTreeNear(t, pricingMust(b.NPV()), cached, row.CurrencyTolerance)
	}
	for _, domain := range []struct{ evaluation, reference, maturity Date }{
		{testDate(t, 7, 7, 2026), testDate(t, 20, 7, 2026), testDate(t, 7, 7, 2031)},
		{testDate(t, 5, 6, 2026), testDate(t, 7, 6, 2026), testDate(t, 7, 6, 2026)},
	} {
		domainSettings := pricingMust(s.NewSettings())
		bondCheck(t, domainSettings.SetEvaluationDate(domain.evaluation))
		domainCurve := pricingMust(s.NewFlatForward(domain.reference, .03, dc))
		domainModel := pricingMust(s.NewHullWhite(domainCurve, row.A, row.Sigma))
		domainEngine := pricingMust(s.NewTreeCallableFixedRateBondEngine(domainModel, 100, domainSettings))
		domainBond := pricingMust(s.NewCallableZeroCouponBond(CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: BondConfig{SettlementDays: 2, FaceAmount: 1000, Settings: domainSettings, Calendar: pricingMust(s.NullCalendar())}, MaturityDate: domain.maturity}, DayCounter: dc}))
		bondCheck(t, domainBond.SetTreeEngine(domainEngine))
		for _, read := range []func() (float64, error){domainBond.NPV, domainBond.SettlementValue, domainBond.CleanPrice, domainBond.DirtyPrice} {
			if _, err := read(); err == nil {
				t.Fatal("invalid tree pricing dates bypassed native calculation")
			}
		}
	}
	bondCheck(t, engine.Close())
	if err := set(engine); err == nil {
		t.Fatal("closed engine accepted")
	}
	bondCheck(t, model.Close())
	if _, err := s.NewTreeCallableFixedRateBondEngine(model, 100, settings); err == nil {
		t.Fatal("closed model accepted")
	}
	bondCheck(t, curve.Close())
	bondCheck(t, dc.Close())
	var nilFixed *CallableFixedRateBond
	if err := nilFixed.SetTreeEngine(wrong); err == nil {
		t.Fatal("nil receiver accepted")
	}
	if err := (&CallableZeroCouponBond{}).SetTreeEngine(wrong); err == nil {
		t.Fatal("zero receiver accepted")
	}
}
func TestCallableTreeConcurrentCallers(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	row := callableTreeRows(t)[1]
	b, _, _, _, _, _, _, _ := callableTreeFixture(t, s, row)
	want := pricingMust(b.NPV())
	var group sync.WaitGroup
	failures := make(chan error, 6)
	for i := 0; i < 6; i++ {
		group.Add(1)
		go func() {
			defer group.Done()
			for j := 0; j < 10; j++ {
				got, err := b.NPV()
				if err != nil {
					failures <- err
					return
				}
				if math.Abs(got-want) > row.CurrencyTolerance {
					failures <- &Error{Message: "tree price changed"}
					return
				}
			}
		}()
	}
	group.Wait()
	close(failures)
	for err := range failures {
		t.Fatal(err)
	}
}
func TestCallableTreeExistingModelCalibrationInvalidatesCache(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	row := callableTreeRows(t)[1]
	b, _, _, model, curve, _, settings, dc := callableTreeFixture(t, s, row)
	initial := pricingMust(b.NPV())
	index := pricingMust(s.NewEuriborSixMonths(curve, settings))
	vol := pricingMust(s.NewSimpleQuote(.01))
	helpers := []*CapHelper{}
	for _, years := range []int32{2, 3, 4, 5} {
		helpers = append(helpers, pricingMust(s.NewCapHelper(CapHelperConfig{Length: Period{years, Years}, Volatility: vol, Index: index, FixedLegFrequency: Annual, FixedLegDayCounter: dc, Curve: curve, ErrorType: RelativePriceError, VolatilityType: Normal})))
	}
	method := pricingMust(s.NewLevenbergMarquardt(nil))
	criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 1000, MaxStationaryStateIterations: pricingPtr(uint(100)), RootEpsilon: 1e-8, FunctionEpsilon: 1e-8, GradientNormEpsilon: pricingPtr(1e-8)}))
	bondCheck(t, model.CalibrateCaps(helpers, method, criteria, true, 30))
	if pricingMust(b.IsCalculated()) {
		t.Fatal("existing model calibration failed to invalidate actual tree cache")
	}
	changed := pricingMust(b.NPV())
	if changed == initial {
		t.Fatal("fitted model ignored")
	}
	freshModel := pricingMust(s.NewHullWhite(curve, pricingMust(model.A()), pricingMust(model.Sigma())))
	freshEngine := pricingMust(s.NewTreeCallableFixedRateBondEngine(freshModel, row.Steps, settings))
	calendar := pricingMust(s.NullCalendar())
	issue := pricingMust(DateFromSerial(row.Issue))
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: pricingMust(DateFromSerial(row.Start)), End: pricingMust(DateFromSerial(row.Maturity)), Frequency: Semiannual, Calendar: calendar, Convention: Unadjusted, Rule: &row.ScheduleRule}))
	calls := make(CallabilitySchedule, len(row.Events))
	for i, event := range row.Events {
		calls[i] = pricingMust(NewCallability(event[0], BondPriceType(event[1]), CallabilityType(event[2]), pricingMust(DateFromSerial(int32(event[3])))))
	}
	fresh := pricingMust(s.NewCallableFixedRateBond(CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: BondConfig{SettlementDays: row.SettlementDays, FaceAmount: row.Face, Settings: settings, IssueDate: &issue, Redemption: &row.Redemption, PaymentConvention: &row.PaymentConvention}, Schedule: schedule, Coupons: row.Coupons, AccrualDayCounter: dc}, Callability: calls}))
	bondCheck(t, fresh.SetTreeEngine(freshEngine))
	callableTreeNear(t, pricingMust(fresh.NPV()), changed, row.CurrencyTolerance)
	bondCheck(t, b.Recalculate())
	callableTreeNear(t, pricingMust(b.NPV()), changed, row.CurrencyTolerance)
}
