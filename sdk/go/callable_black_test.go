package itofin

import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"sync"
	"testing"
)

type callableBlackSnapshot struct {
	NPV             float64 `json:"npv"`
	SettlementValue float64 `json:"settlement_value"`
	Clean           float64 `json:"clean_price"`
	Dirty           float64 `json:"dirty_price"`
	Accrued         float64 `json:"accrued"`
	Settlement      int32   `json:"settlement"`
}
type callableBlackCase struct {
	IVUnidentifiable  bool                   `json:"iv_known_vol_unidentifiable"`
	Name              string                 `json:"name"`
	Kind              string                 `json:"kind"`
	Reference         int32                  `json:"reference"`
	Evaluation        int32                  `json:"evaluation"`
	Start             int32                  `json:"start"`
	Maturity          int32                  `json:"maturity"`
	Issue             int32                  `json:"issue"`
	Exercise          int32                  `json:"exercise"`
	Calendar          string                 `json:"calendar"`
	SettlementDays    uint32                 `json:"settlement_days"`
	Face              float64                `json:"face"`
	Redemption        float64                `json:"redemption"`
	PaymentConvention BusinessDayConvention  `json:"payment_convention"`
	Coupons           []float64              `json:"coupons"`
	ExLength          int32                  `json:"ex_length"`
	ExUnit            TimeUnit               `json:"ex_unit"`
	ExEOM             bool                   `json:"ex_eom"`
	ScheduleRule      DateGeneration         `json:"schedule_rule"`
	Frequency         Frequency              `json:"frequency"`
	DayCounter        int32                  `json:"day_counter"`
	Rate              float64                `json:"rate"`
	Volatility        float64                `json:"volatility"`
	Strike            float64                `json:"strike"`
	PriceType         BondPriceType          `json:"price_type"`
	Put               bool                   `json:"put"`
	CurveDates        []int32                `json:"curve_dates"`
	CurveRates        []float64              `json:"curve_rates"`
	Expected          callableBlackSnapshot  `json:"expected"`
	Lower             *callableBlackSnapshot `json:"lower_endpoint"`
	Upper             *callableBlackSnapshot `json:"upper_endpoint"`
	UpdatedVolatility *callableBlackSnapshot `json:"updated_volatility"`
	UpdatedRate       *callableBlackSnapshot `json:"updated_rate"`
}
type callableBlackFixture struct {
	b        *Bond
	fixed    *CallableFixedRateBond
	zero     *CallableZeroCouponBond
	engine   *BlackCallableFixedRateBondEngine
	curve    *YieldTermStructure
	vol      *SimpleQuote
	rate     *SimpleQuote
	settings *Settings
	dc       *DayCounter
}

func callableBlackRows(t *testing.T) []callableBlackCase {
	t.Helper()
	var oracle struct {
		Version string              `json:"quantlib_version"`
		Cases   []callableBlackCase `json:"cases"`
	}
	bondCheck(t, json.Unmarshal(pricingMust(os.ReadFile("testdata/callable_black_oracle.json")), &oracle))
	if oracle.Version != "1.43" || len(oracle.Cases) != 59 {
		t.Fatal("incomplete independent Black oracle")
	}
	expected := map[string]bool{}
	strict := map[string]bool{}
	for _, direction := range []string{"call", "put"} {
		for _, lag := range []int{0, 3, 15} {
			name := fmt.Sprintf("zero_%s_lag%d_vol0.1", direction, lag)
			expected[name] = true
			strict[name+"_strike85"] = true
		}
	}
	qualified := 0
	strictCount := 0
	for _, row := range oracle.Cases {
		if row.IVUnidentifiable {
			if !expected[row.Name] {
				t.Fatal("unapproved IV qualification", row.Name)
			}
			delete(expected, row.Name)
			qualified++
		}
		if strict[row.Name] {
			if row.IVUnidentifiable || row.Strike != 85 || row.Volatility != .1 {
				t.Fatal("strict strike85 case weakened")
			}
			delete(strict, row.Name)
			strictCount++
		}
	}
	if qualified != 6 || strictCount != 6 || len(expected) != 0 || len(strict) != 0 {
		t.Fatal("qualification/strike85 set drift")
	}
	return oracle.Cases
}
func (f callableBlackFixture) set(engine *BlackCallableFixedRateBondEngine) error {
	if f.fixed != nil {
		return f.fixed.SetBlackEngine(engine)
	}
	return f.zero.SetBlackEngine(engine)
}
func (f callableBlackFixture) tree(engine *TreeCallableFixedRateBondEngine) error {
	if f.fixed != nil {
		return f.fixed.SetTreeEngine(engine)
	}
	return f.zero.SetTreeEngine(engine)
}
func (f callableBlackFixture) iv(target float64, curve *YieldTermStructure, options *CallableImpliedVolatilityOptions) (float64, error) {
	if f.fixed != nil {
		return f.fixed.ImpliedVolatility(target, curve, options)
	}
	return f.zero.ImpliedVolatility(target, curve, options)
}
func blackFixture(t *testing.T, s *Session, row callableBlackCase, attach bool, override *CallabilitySchedule) callableBlackFixture {
	t.Helper()
	f := callableBlackFixture{}
	f.settings = pricingMust(s.NewSettings())
	bondCheck(t, f.settings.SetEvaluationDate(pricingMust(DateFromSerial(row.Evaluation))))
	cal := pricingMust(s.NullCalendar())
	if row.Calendar == "target" {
		bondCheck(t, cal.Close())
		cal = pricingMust(s.Target())
	}
	f.dc = pricingMust(s.Actual365Fixed())
	if row.DayCounter == 0 {
		bondCheck(t, f.dc.Close())
		f.dc = pricingMust(s.Actual360())
	}
	curveDC := pricingMust(s.Actual365Fixed())
	f.rate = pricingMust(s.NewSimpleQuote(row.Rate))
	if len(row.CurveDates) == 0 {
		f.curve = pricingMust(s.NewFlatForwardFromQuote(pricingMust(DateFromSerial(row.Reference)), f.rate, curveDC))
	} else {
		dates := make([]Date, len(row.CurveDates))
		for i, v := range row.CurveDates {
			dates[i] = pricingMust(DateFromSerial(v))
		}
		f.curve = pricingMust(s.NewZeroCurve(NodeCurveConfig{Dates: dates, Values: row.CurveRates, DayCounter: curveDC}))
	}
	bondCheck(t, curveDC.Close())
	f.vol = pricingMust(s.NewSimpleQuote(row.Volatility))
	f.engine = pricingMust(s.NewBlackCallableFixedRateBondEngine(f.vol, f.curve, f.settings))
	kind := CallabilityCall
	if row.Put {
		kind = CallabilityPut
	}
	calls := CallabilitySchedule{pricingMust(NewCallability(row.Strike, row.PriceType, kind, pricingMust(DateFromSerial(row.Exercise))))}
	if override != nil {
		calls = *override
	}
	issue := pricingMust(DateFromSerial(row.Issue))
	config := BondConfig{SettlementDays: row.SettlementDays, FaceAmount: row.Face, Redemption: &row.Redemption, Settings: f.settings, IssueDate: &issue, PaymentConvention: &row.PaymentConvention}
	if row.Kind == "zero" {
		config.Calendar = cal
		f.zero = pricingMust(s.NewCallableZeroCouponBond(CallableZeroCouponBondConfig{ZeroCouponBondConfig: ZeroCouponBondConfig{BondConfig: config, MaturityDate: pricingMust(DateFromSerial(row.Maturity))}, DayCounter: f.dc, Callability: calls}))
		f.b = &f.zero.Bond
	} else {
		schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: pricingMust(DateFromSerial(row.Start)), End: pricingMust(DateFromSerial(row.Maturity)), Frequency: row.Frequency, Calendar: cal, Convention: Unadjusted, Rule: &row.ScheduleRule}))
		cfg := CallableFixedRateBondConfig{FixedRateBondConfig: FixedRateBondConfig{BondConfig: config, Schedule: schedule, Coupons: row.Coupons, AccrualDayCounter: f.dc, ExCouponEndOfMonth: row.ExEOM}, Callability: calls}
		if row.ExLength != 0 {
			cfg.ExCouponPeriod = &Period{row.ExLength, row.ExUnit}
		}
		f.fixed = pricingMust(s.NewCallableFixedRateBond(cfg))
		f.b = &f.fixed.Bond
		bondCheck(t, schedule.Close())
	}
	if attach {
		bondCheck(t, f.set(f.engine))
	}
	bondCheck(t, cal.Close())
	return f
}
func blackAssert(t *testing.T, f callableBlackFixture, row callableBlackCase, want callableBlackSnapshot) {
	t.Helper()
	for _, p := range []struct {
		read     func() (float64, error)
		want     float64
		currency bool
	}{{f.b.NPV, want.NPV, true}, {f.b.SettlementValue, want.SettlementValue, true}, {f.b.CleanPrice, want.Clean, false}, {f.b.DirtyPrice, want.Dirty, false}} {
		got := pricingMust(p.read())
		if p.currency && row.Face == 1000 {
			callableTreeNear(t, got, p.want, 1e-8)
		} else if p.currency {
			callableTreeNear(t, got/row.Face*100, p.want/row.Face*100, 1e-9)
		} else {
			callableTreeNear(t, got, p.want, 1e-9)
		}
	}
	callableTreeNear(t, pricingMust(f.b.AccruedAmount(nil)), want.Accrued, 1e-14)
	if pricingMust(f.b.SettlementDate(nil)).Serial() != want.Settlement || !pricingMust(f.b.IsCalculated()) {
		t.Fatal("Black settlement/shared cache")
	}
}
func blackError(t *testing.T, err error, code int32) {
	t.Helper()
	var native *Error
	if !errors.As(err, &native) || native.Code != code || native.Message == "" {
		t.Fatalf("wanted native code%d, got %v", code, err)
	}
}
func TestCallableBlackQuantLibOracles(t *testing.T) {
	for _, row := range callableBlackRows(t) {
		t.Run(row.Name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			f := blackFixture(t, s, row, true, nil)
			blackAssert(t, f, row, row.Expected)
		})
	}
}
func TestCallableBlackRetainedLifecycleAndReplacement(t *testing.T) {
	for _, kind := range []string{"fixed", "zero"} {
		t.Run(kind, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			row := callableBlackRows(t)[4]
			row.Kind = kind
			f := blackFixture(t, s, row, true, nil)
			old := pricingMust(f.b.NPV())
			bondCheck(t, f.vol.SetValue(.42))
			if pricingMust(f.b.IsCalculated()) {
				t.Fatal("vol quote did not invalidate")
			}
			changed := pricingMust(f.b.NPV())
			if changed == old {
				t.Fatal("live Black volatility detached")
			}
			bondCheck(t, f.b.Freeze())
			bondCheck(t, f.rate.SetValue(.051))
			if pricingMust(f.b.NPV()) != changed {
				t.Fatal("freeze lost cached value")
			}
			bondCheck(t, f.b.Recalculate())
			recalculated := pricingMust(f.b.NPV())
			if recalculated == changed {
				t.Fatal("frozen recalculation ignored input")
			}
			bondCheck(t, f.rate.SetValue(.04))
			if pricingMust(f.b.NPV()) != recalculated {
				t.Fatal("recalculate unfroze")
			}
			bondCheck(t, f.b.Unfreeze())
			pricingMust(f.b.NPV())
			otherVol := pricingMust(s.NewSimpleQuote(.2))
			otherCurve := pricingMust(s.NewFlatForward(pricingMust(DateFromSerial(row.Reference)), .035, f.dc))
			otherEngine := pricingMust(s.NewBlackCallableFixedRateBondEngine(otherVol, otherCurve, f.settings))
			bondCheck(t, f.set(otherEngine))
			pricingMust(f.b.NPV())
			bondCheck(t, f.vol.SetValue(.35))
			bondCheck(t, f.rate.SetValue(.041))
			if !pricingMust(f.b.IsCalculated()) {
				t.Fatal("old Black graph still attached")
			}
			bondCheck(t, otherVol.SetValue(.3))
			if pricingMust(f.b.IsCalculated()) {
				t.Fatal("new Black graph detached")
			}
			pricingMust(f.b.NPV())
			model := pricingMust(s.NewHullWhite(f.curve, .1, .01))
			tree := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, 100, f.settings))
			bondCheck(t, f.tree(tree))
			pricingMust(f.b.NPV())
			bondCheck(t, otherVol.SetValue(.4))
			if !pricingMust(f.b.IsCalculated()) {
				t.Fatal("Black graph attached after Tree replacement")
			}
			bondCheck(t, f.set(f.engine))
			pricingMust(f.b.NPV())
			bondCheck(t, f.rate.SetValue(.04))
			if pricingMust(f.b.IsCalculated()) {
				t.Fatal("installed Black curve did not invalidate")
			}
			pricingMust(f.b.NPV())
			bondCheck(t, f.engine.Close())
			bondCheck(t, f.vol.Close())
			bondCheck(t, f.curve.Close())
			bondCheck(t, f.rate.Close())
			bondCheck(t, f.dc.Close())
			bondCheck(t, f.settings.Close())
			bondCheck(t, f.b.Recalculate())
			pricingMust(f.b.NPV())
			bondCheck(t, f.b.Close())
			if _, err := f.b.NPV(); err == nil {
				t.Fatal("closed bond priced")
			}
		})
	}
}
func TestCallableBlackValidationAndSessions(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	s2 := pricingMust(NewSession())
	defer s2.Close()
	row := callableBlackRows(t)[4]
	f := blackFixture(t, s, row, true, nil)
	for _, call := range []func() error{func() error { _, e := s.NewBlackCallableFixedRateBondEngine(nil, f.curve, f.settings); return e }, func() error { _, e := s.NewBlackCallableFixedRateBondEngine(f.vol, nil, f.settings); return e }, func() error { _, e := s.NewBlackCallableFixedRateBondEngine(f.vol, f.curve, nil); return e }, func() error { return f.set(nil) }} {
		if call() == nil {
			t.Fatal("nil input accepted")
		}
	}
	other := pricingMust(s.NewSettings())
	bondCheck(t, other.SetEvaluationDate(pricingMust(DateFromSerial(row.Evaluation))))
	wrong := pricingMust(s.NewBlackCallableFixedRateBondEngine(f.vol, f.curve, other))
	cached := pricingMust(f.b.NPV())
	if f.set(wrong) == nil {
		t.Fatal("settings allocation mismatch accepted")
	}
	if !pricingMust(f.b.IsCalculated()) || pricingMust(f.b.NPV()) != cached {
		t.Fatal("failed setter changed cache")
	}
	foreign := pricingMust(s2.NewSimpleQuote(.2))
	if _, err := s.NewBlackCallableFixedRateBondEngine(foreign, f.curve, f.settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("foreign quote", err)
	}
	foreignSettings := pricingMust(s2.NewSettings())
	foreignDC := pricingMust(s2.Actual365Fixed())
	foreignCurve := pricingMust(s2.NewFlatForward(pricingMust(DateFromSerial(row.Reference)), .04, foreignDC))
	foreignEngine := pricingMust(s2.NewBlackCallableFixedRateBondEngine(foreign, foreignCurve, foreignSettings))
	if err := f.set(foreignEngine); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("foreign engine", err)
	}
	if _, err := f.iv(row.Expected.Clean, foreignCurve, nil); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("foreign IV curve", err)
	}
	for _, v := range []float64{-.1, math.NaN(), math.Inf(1)} {
		bondCheck(t, f.vol.SetValue(v))
		deferred := pricingMust(s.NewBlackCallableFixedRateBondEngine(f.vol, f.curve, f.settings))
		bondCheck(t, deferred.Close())
		if _, err := f.b.NPV(); err == nil {
			t.Fatal("invalid live volatility valued")
		}
		bondCheck(t, f.vol.SetValue(row.Volatility))
		callableTreeNear(t, pricingMust(f.b.NPV()), cached, 1e-8)
	}
	for _, v := range []float64{math.NaN(), math.Inf(1), 1000} {
		bondCheck(t, f.rate.SetValue(v))
		pricingMust(s.NewBlackCallableFixedRateBondEngine(f.vol, f.curve, f.settings))
		if _, err := f.b.NPV(); err == nil {
			t.Fatal("invalid discount valued")
		}
		bondCheck(t, f.rate.SetValue(row.Rate))
		pricingMust(f.b.NPV())
	}
	for _, date := range []int32{row.Evaluation, row.Expected.Settlement, row.Maturity} {
		call := pricingMust(NewCallability(100, Clean, CallabilityCall, pricingMust(DateFromSerial(date))))
		g := blackFixture(t, s, row, true, &CallabilitySchedule{call})
		if _, err := g.b.NPV(); err == nil {
			t.Fatal("inactive/redemption exercise valued")
		}
		if _, err := g.iv(row.Expected.Clean, g.curve, nil); err == nil {
			t.Fatal("unsupported right IV")
		}
	}
	past := pricingMust(NewCallability(100, Clean, CallabilityCall, pricingMust(DateFromSerial(row.Evaluation-1))))
	future := pricingMust(NewCallability(100, Clean, CallabilityPut, pricingMust(DateFromSerial(row.Exercise))))
	for _, calls := range []CallabilitySchedule{{}, {past, future}, {future, future}} {
		g := blackFixture(t, s, row, true, &calls)
		if _, err := g.b.NPV(); err == nil {
			t.Fatal("original right count pruned")
		}
	}
	negativeRow := row
	negativeRow.Kind = "zero"
	negativeRow.Rate = -.01
	negativeRow.Volatility = 0
	negativeFixture := blackFixture(t, s, negativeRow, true, nil)
	pricingMust(negativeFixture.b.NPV())
	bondCheck(t, negativeFixture.vol.SetValue(.2))
	_, err := negativeFixture.b.NPV()
	blackError(t, err, 3)
	zeroRedemption := row
	zeroRedemption.Kind = "zero"
	zeroRedemption.Redemption = 0
	zeroFixture := blackFixture(t, s, zeroRedemption, true, nil)
	for _, read := range []func() (float64, error){zeroFixture.b.NPV, zeroFixture.b.SettlementValue, zeroFixture.b.CleanPrice, zeroFixture.b.DirtyPrice} {
		_, err := read()
		blackError(t, err, 3)
	}
	domainRow := row
	domainRow.Reference = row.Evaluation + 20
	domainFixture := blackFixture(t, s, domainRow, true, nil)
	_, err = domainFixture.b.NPV()
	blackError(t, err, 3)
	overflowFixture := blackFixture(t, s, row, false, nil)
	bondCheck(t, overflowFixture.settings.SetEvaluationDate(testDate(t, 31, 12, 2199)))
	_, err = overflowFixture.iv(row.Expected.Clean, overflowFixture.curve, nil)
	blackError(t, err, 1)
	closedQuote := pricingMust(s.NewSimpleQuote(.2))
	bondCheck(t, closedQuote.Close())
	_, err = s.NewBlackCallableFixedRateBondEngine(closedQuote, f.curve, f.settings)
	blackError(t, err, 2)
	closedCurve := pricingMust(s.NewFlatForward(pricingMust(DateFromSerial(row.Reference)), .04, f.dc))
	bondCheck(t, closedCurve.Close())
	_, err = f.iv(100, closedCurve, nil)
	blackError(t, err, 2)
	bondCheck(t, f.settings.SetEvaluationDate(pricingMust(DateFromSerial(row.Maturity+1))))
	callableTreeNear(t, pricingMust(f.b.NPV()), 0, 0)
	if _, err := f.iv(row.Expected.Clean, f.curve, nil); err == nil {
		t.Fatal("expired IV accepted")
	}
	bondCheck(t, f.engine.Close())
	if f.set(f.engine) == nil {
		t.Fatal("released engine accepted")
	}
	var nilFixed *CallableFixedRateBond
	if nilFixed.SetBlackEngine(wrong) == nil {
		t.Fatal("nil receiver")
	}
	if _, err := nilFixed.ImpliedVolatility(100, f.curve, nil); err == nil {
		t.Fatal("nil IV receiver")
	}
	if (&CallableZeroCouponBond{}).SetBlackEngine(wrong) == nil {
		t.Fatal("zero receiver")
	}
}
func TestCallableBlackImpliedVolatilityRoundTripsAndBounds(t *testing.T) {
	for _, row := range callableBlackRows(t) {
		t.Run(row.Name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			f := blackFixture(t, s, row, false, nil)
			endpoint := blackFixture(t, s, row, true, nil)
			for _, kind := range []BondPriceType{Clean, Dirty} {
				target := row.Expected.Clean
				if kind == Dirty {
					target = row.Expected.Dirty
				}
				want := row.Volatility
				if row.Lower != nil && target == row.Lower.Clean && kind == Clean {
					want = 0
				}
				if row.Lower != nil && target == row.Lower.Dirty && kind == Dirty {
					want = 0
				}
				if want == 0 {
					bondCheck(t, endpoint.vol.SetValue(0))
					target = pricingMust(endpoint.b.DirtyPrice())
					kind = Dirty
				}
				got := pricingMust(f.iv(target, f.curve, &CallableImpliedVolatilityOptions{PriceType: kind}))
				if row.IVUnidentifiable {
					if math.IsNaN(got) || got < 0 || got > 1 {
						t.Fatal("unidentifiable IV outside bracket")
					}
					bondCheck(t, endpoint.vol.SetValue(got))
					callableTreeNear(t, pricingMust(endpoint.b.CleanPrice()), row.Expected.Clean, 1e-9)
					callableTreeNear(t, pricingMust(endpoint.b.DirtyPrice()), row.Expected.Dirty, 1e-9)
				} else {
					callableTreeNear(t, got, want, 2e-8)
				}
			}
			if pricingMust(f.b.IsCalculated()) {
				t.Fatal("IV populated installed cache")
			}
			if row.Lower != nil {
				one := uint(1)
				bondCheck(t, endpoint.vol.SetValue(0))
				target := pricingMust(endpoint.b.DirtyPrice())
				callableTreeNear(t, target, row.Lower.Dirty, 1e-9)
				callableTreeNear(t, pricingMust(f.iv(target, f.curve, &CallableImpliedVolatilityOptions{PriceType: Dirty, MaxEvaluations: &one})), 0, 0)
			}
			if row.Upper != nil && row.Upper.Clean != row.Lower.Clean {
				two := uint(2)
				bondCheck(t, endpoint.vol.SetValue(1))
				target := pricingMust(endpoint.b.DirtyPrice())
				callableTreeNear(t, target, row.Upper.Dirty, 1e-9)
				callableTreeNear(t, pricingMust(f.iv(target, f.curve, &CallableImpliedVolatilityOptions{PriceType: Dirty, MaxEvaluations: &two})), 1, 0)
			}
		})
	}
	s := pricingMust(NewSession())
	defer s.Close()
	row := callableBlackRows(t)[5]
	f := blackFixture(t, s, row, false, nil)
	zero := 0.
	one := 1.
	negative := -1.
	nan := math.NaN()
	budget0 := uint(0)
	budget1 := uint(1)
	for _, a := range []*CallableImpliedVolatilityOptions{{Accuracy: &zero}, {Accuracy: &negative}, {Accuracy: &nan}, {MaxEvaluations: &budget0}, {MinVol: &negative}, {MaxVol: &zero}, {MinVol: &one, MaxVol: &one}, {MaxVol: &nan}, {PriceType: 99}} {
		_, err := f.iv(row.Expected.Clean, f.curve, a)
		blackError(t, err, 1)
	}
	for _, target := range []float64{math.NaN(), math.Inf(1), -1000, 10000} {
		_, err := f.iv(target, f.curve, nil)
		code := int32(3)
		if math.IsNaN(target) || math.IsInf(target, 0) {
			code = 1
		}
		blackError(t, err, code)
	}
	if _, err := f.iv(row.Expected.Clean, f.curve, &CallableImpliedVolatilityOptions{MaxEvaluations: &budget1}); err == nil {
		t.Fatal("interior budget ignored")
	}
	if _, err := f.iv(row.Upper.Clean, f.curve, &CallableImpliedVolatilityOptions{MaxEvaluations: &budget1}); err == nil {
		t.Fatal("upper endpoint budget ignored")
	}
	if _, err := f.iv(row.Expected.Clean, nil, nil); err == nil {
		t.Fatal("nil discount")
	}
}
func TestCallableBlackImpliedVolatilityPreservesInstalledState(t *testing.T) {
	for _, kind := range []string{"fixed", "zero"} {
		t.Run(kind, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			row := callableBlackRows(t)[5]
			row.Kind = kind
			f := blackFixture(t, s, row, true, nil)
			target := pricingMust(f.b.CleanPrice())
			cached := pricingMust(f.b.NPV())
			bondCheck(t, f.b.Freeze())
			model := pricingMust(s.NewHullWhite(f.curve, .1, .01))
			tree := pricingMust(s.NewTreeCallableFixedRateBondEngine(model, 100, f.settings))
			for _, useTree := range []bool{false, true} {
				if useTree {
					bondCheck(t, f.tree(tree))
					bondCheck(t, f.b.Recalculate())
					cached = pricingMust(f.b.NPV())
				}
				if _, err := f.iv(target, f.curve, nil); err != nil {
					t.Fatal(err)
				}
				budget := uint(1)
				for _, args := range []struct {
					target float64
					a      *CallableImpliedVolatilityOptions
				}{{10000, nil}, {target, &CallableImpliedVolatilityOptions{MaxEvaluations: &budget}}} {
					if _, err := f.iv(args.target, f.curve, args.a); err == nil {
						t.Fatal("failure test unexpectedly succeeded")
					}
				}
				if !pricingMust(f.b.IsCalculated()) || pricingMust(f.b.NPV()) != cached {
					t.Fatal("IV changed installed frozen cache")
				}
			}
			bondCheck(t, f.rate.SetValue(.05))
			if pricingMust(f.b.NPV()) != cached {
				t.Fatal("IV unfroze instrument")
			}
			bondCheck(t, f.b.Unfreeze())
			if pricingMust(f.b.NPV()) == cached {
				t.Fatal("installed live graph detached by IV")
			}
		})
	}
}
func TestCallableBlackConcurrentCallers(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	row := callableBlackRows(t)[5]
	f := blackFixture(t, s, row, true, nil)
	want := pricingMust(f.b.NPV())
	var group sync.WaitGroup
	failures := make(chan error, 6)
	for i := 0; i < 6; i++ {
		group.Add(1)
		go func() {
			defer group.Done()
			for j := 0; j < 8; j++ {
				got, err := f.b.NPV()
				if err != nil || got != want {
					failures <- fmt.Errorf("concurrent NPV %v %v", got, err)
					return
				}
				vol, err := f.iv(row.Expected.Clean, f.curve, nil)
				if err != nil || math.Abs(vol-row.Volatility) > 2e-8 {
					failures <- fmt.Errorf("concurrent IV %v %v", vol, err)
					return
				}
			}
		}()
	}
	group.Wait()
	close(failures)
	for err := range failures {
		t.Error(err)
	}
}
