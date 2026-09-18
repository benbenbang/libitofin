package itofin

import (
	"encoding/csv"
	"errors"
	"fmt"
	"math"
	"os"
	"runtime"
	"strconv"
	"strings"
	"testing"
)

func globalStrip(t *testing.T) (*Session, *Settings, []*SimpleQuote, PiecewiseCurveConfig) {
	t.Helper()
	s := sessionMust(NewSession())
	t.Cleanup(func() { _ = s.Close() })
	settings := sessionMust(s.NewSettings())
	if err := settings.SetEvaluationDate(curveDate(t, 15, 6, 2026)); err != nil {
		t.Fatal(err)
	}
	cfg := PiecewiseCurveConfig{ReferenceDate: curveDate(t, 17, 6, 2026), DayCounter: sessionMust(s.Actual365Fixed()), Bootstrap: "global"}
	quotes := []*SimpleQuote{sessionMust(s.NewSimpleQuote(.04)), sessionMust(s.NewSimpleQuote(.045))}
	for i, months := range []int32{1, 3} {
		index := sessionMust(s.NewEuribor(Period{months, Months}, nil, settings))
		cfg.Helpers = append(cfg.Helpers, sessionMust(s.NewDepositRateHelper(quotes[i], index)))
	}
	return s, settings, quotes, cfg
}

func TestGlobalBootstrapIndependentOracleAndRetainedGraph(t *testing.T) {
	s, settings, quotes, cfg := globalStrip(t)
	convexity := sessionMust(s.NewSimpleQuote(.01))
	end := curveDate(t, 17, 9, 2026)
	future := sessionMust(s.NewFuturesRateHelperFromEndDate(FuturesRateHelperConfig{
		Price: sessionMust(s.NewSimpleQuote(95)), IborStartDate: cfg.ReferenceDate, IborEndDate: &end,
		DayCounter: sessionMust(s.Actual360()), ConvexityAdjustment: convexity,
		FuturesType: Custom, DoNotObserveConvexity: true,
	}))
	cfg.AdditionalHelpers = cfg.Helpers[1:]
	cfg.Helpers = []*RateHelper{cfg.Helpers[0], future}
	variables := sessionMust(s.NewSimpleQuoteVariables([]*SimpleQuote{convexity}, []float64{.01}, []float64{0}))
	cfg.AdditionalVariables = variables
	dateCalls, penaltyCalls := 0, 0
	august := curveDate(t, 17, 8, 2026)
	cfg.AdditionalDates = func() ([]Date, error) {
		dateCalls++
		return []Date{august}, nil
	}
	cfg.AdditionalPenalties = func(state BootstrapState) ([]float64, error) {
		penaltyCalls++
		if len(state.Times) != 4 || len(state.Data) != 4 || len(state.QuoteValues) != 1 || len(state.HelperErrors) != 1 {
			return nil, fmt.Errorf("unexpected callback state: %+v", state)
		}
		if state.Times[0] != 0 || state.Data[0] != 1 || !isFiniteGlobal(state.QuoteValues[0]) {
			return nil, fmt.Errorf("invalid callback state: %+v", state)
		}
		return []float64{1e4 * state.HelperErrors[0], state.Data[2] - .99}, nil
	}
	curve := sessionMust(s.NewPiecewiseYieldCurve(cfg))
	if err := variables.Close(); err != nil {
		t.Fatal(err)
	}
	source, err := os.Open("testdata/global_bootstrap_oracle.csv")
	if err != nil {
		t.Fatal(err)
	}
	defer source.Close()
	rows, err := csv.NewReader(source).ReadAll()
	if err != nil || len(rows) != 3 {
		t.Fatalf("oracle rows=%d error=%v", len(rows), err)
	}
	for _, row := range rows[1:] {
		values := make([]float64, len(row))
		for i, text := range row {
			values[i], err = strconv.ParseFloat(text, 64)
			if err != nil {
				t.Fatal(err)
			}
		}
		before := dateCalls
		if err = quotes[1].SetValue(values[0]); err != nil {
			t.Fatal(err)
		}
		for i, month := range []int{7, 8, 9} {
			got, err := curve.DiscountDate(curveDate(t, 17, month, 2026), false)
			curveNear(t, curveMust(t, got, err), values[i+1], 1e-9)
		}
		got, err := convexity.Value()
		curveNear(t, curveMust(t, got, err), values[4], 1e-9)
		got, err = future.ImpliedQuote()
		curveNear(t, curveMust(t, got, err), 95, 1e-9)
		got, err = cfg.AdditionalHelpers[0].QuoteError()
		curveNear(t, curveMust(t, got, err), 0, 1e-9)
		if dateCalls <= before || penaltyCalls == 0 {
			t.Fatal("callbacks did not run during calibration")
		}
	}
	retained := sessionMust(s.NewEuribor(Period{3, Months}, curve, settings))
	for _, obj := range []interface{ Close() error }{curve, convexity, future, cfg.Helpers[0], cfg.AdditionalHelpers[0]} {
		if err := obj.Close(); err != nil {
			t.Fatal(err)
		}
	}
	cfg = PiecewiseCurveConfig{}
	runtime.GC()
	before := dateCalls
	if err := quotes[1].SetValue(.041); err != nil {
		t.Fatal(err)
	}
	fixing, err := retained.Fixing(curveDate(t, 15, 6, 2026), false)
	curveNear(t, curveMust(t, fixing, err), .041, 1e-9)
	if dateCalls <= before {
		t.Fatal("retained curve did not recalibrate")
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := retained.Fixing(curveDate(t, 15, 6, 2026), false); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session retained graph: %v", err)
	}
}

func isFiniteGlobal(v float64) bool { return !math.IsNaN(v) && !math.IsInf(v, 0) }

func TestGlobalBootstrapCallbackErrorsAndReentry(t *testing.T) {
	for _, phase := range []string{"dates", "penalties"} {
		for _, failure := range []string{"error", "panic", "query", "close"} {
			t.Run(phase+"/"+failure, func(t *testing.T) {
				s, _, quotes, cfg := globalStrip(t)
				called := false
				callback := func() error {
					called = true
					switch failure {
					case "error":
						return errors.New("deliberate callback failure")
					case "panic":
						panic("deliberate callback failure")
					case "query":
						_, err := quotes[0].Value()
						if !errors.Is(err, ErrCallbackReentry) {
							return fmt.Errorf("wrong reentry error: %v", err)
						}
					case "close":
						if err := s.Close(); !errors.Is(err, ErrCallbackReentry) {
							return fmt.Errorf("wrong close reentry error: %v", err)
						}
					}
					return errors.New("deliberate callback failure")
				}
				if phase == "dates" {
					cfg.AdditionalDates = func() ([]Date, error) { return nil, callback() }
				} else {
					cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) { return nil, callback() }
				}
				curve, err := s.NewPiecewiseYieldCurve(cfg)
				if err == nil {
					_, err = curve.Discount(.05, false)
				}
				if !called || err == nil || !strings.Contains(err.Error(), "deliberate callback failure") {
					t.Fatalf("called=%v error=%v", called, err)
				}
				got, err := quotes[0].Value()
				curveNear(t, curveMust(t, got, err), .04, 0)
			})
		}
	}
}

func TestGlobalBootstrapVariablesValidation(t *testing.T) {
	s, _, quotes, cfg := globalStrip(t)
	for _, tc := range []struct{ guesses, bounds []float64 }{
		{[]float64{1, 2}, nil}, {nil, []float64{0, 1}}, {[]float64{0}, []float64{0}},
		{[]float64{math.NaN()}, nil}, {[]float64{1}, []float64{math.Inf(1)}},
	} {
		if _, err := s.NewSimpleQuoteVariables(quotes[:1], tc.guesses, tc.bounds); err == nil {
			t.Fatalf("invalid variables accepted: %+v", tc)
		}
	}
	if _, err := s.NewSimpleQuoteVariables([]*SimpleQuote{nil}, nil, nil); err == nil {
		t.Fatal("nil quote accepted")
	}
	other := sessionMust(NewSession())
	defer other.Close()
	foreign := sessionMust(other.NewSimpleQuote(.1))
	if _, err := s.NewSimpleQuoteVariables([]*SimpleQuote{foreign}, nil, nil); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign quote: %v", err)
	}
	variables := sessionMust(other.NewSimpleQuoteVariables([]*SimpleQuote{foreign}, nil, nil))
	cfg.AdditionalVariables = variables
	if _, err := s.NewPiecewiseYieldCurve(cfg); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign variables: %v", err)
	}
	local := sessionMust(s.NewSimpleQuote(.1))
	variables = sessionMust(s.NewSimpleQuoteVariables([]*SimpleQuote{local}, nil, nil))
	if err := variables.Close(); err != nil {
		t.Fatal(err)
	}
	cfg.AdditionalVariables = variables
	if _, err := s.NewPiecewiseYieldCurve(cfg); err == nil {
		t.Fatal("released variables accepted")
	}
	if err := local.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.NewSimpleQuoteVariables([]*SimpleQuote{local}, nil, nil); err == nil {
		t.Fatal("released quote accepted")
	}
}

func TestGlobalBootstrapOptionsRequireGlobalAndEnoughResiduals(t *testing.T) {
	for _, option := range []string{"dates", "penalties", "variables"} {
		t.Run(option, func(t *testing.T) {
			s, _, quotes, cfg := globalStrip(t)
			cfg.Bootstrap = "iterative"
			switch option {
			case "dates":
				cfg.AdditionalDates = func() ([]Date, error) { return nil, nil }
			case "penalties":
				cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) { return nil, nil }
			case "variables":
				cfg.AdditionalVariables = sessionMust(s.NewSimpleQuoteVariables(quotes[:1], nil, nil))
			}
			if _, err := s.NewPiecewiseYieldCurve(cfg); err == nil {
				t.Fatal("iterative bootstrap accepted a global option")
			}
		})
	}
	s, _, _, cfg := globalStrip(t)
	august := curveDate(t, 17, 8, 2026)
	cfg.AdditionalDates = func() ([]Date, error) { return []Date{august}, nil }
	curve := sessionMust(s.NewPiecewiseYieldCurve(cfg))
	if _, err := curve.Discount(.05, false); err == nil || !strings.Contains(err.Error(), "less functions") {
		t.Fatalf("underdetermined bootstrap: %v", err)
	}
}

func TestFuturesConvexityObserverSelection(t *testing.T) {
	for _, disabled := range []bool{false, true} {
		t.Run(fmt.Sprintf("disabled=%v", disabled), func(t *testing.T) {
			s := sessionMust(NewSession())
			defer s.Close()
			start, end := curveDate(t, 17, 6, 2026), curveDate(t, 17, 9, 2026)
			convexity := sessionMust(s.NewSimpleQuote(.01))
			helper := sessionMust(s.NewFuturesRateHelperFromEndDate(FuturesRateHelperConfig{
				Price: sessionMust(s.NewSimpleQuote(95)), IborStartDate: start, IborEndDate: &end,
				DayCounter: sessionMust(s.Actual360()), ConvexityAdjustment: convexity,
				FuturesType: Custom, DoNotObserveConvexity: disabled,
			}))
			curve := sessionMust(s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{
				ReferenceDate: start, Helpers: []*RateHelper{helper}, DayCounter: sessionMust(s.Actual365Fixed()),
			}))
			before, err := curve.DiscountDate(end, false)
			curveNear(t, curveMust(t, before, err), 1/(1+.04*92/360), 1e-12)
			if err := convexity.SetValue(.02); err != nil {
				t.Fatal(err)
			}
			after, err := curve.DiscountDate(end, false)
			want := 1 / (1 + .03*92/360)
			if disabled {
				want = before
			}
			curveNear(t, curveMust(t, after, err), want, 1e-12)
		})
	}
}
