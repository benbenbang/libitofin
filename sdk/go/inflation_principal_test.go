package itofin

import (
	"encoding/csv"
	"encoding/json"
	"errors"
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"
)

type principalInputs struct {
	Inputs struct {
		EvaluationDate string `json:"evaluation_date"`
		Fixings        []struct {
			Date  string
			Value float64
		} `json:"historical_fixings"`
		CurveDates   []string `json:"inflation_curve_dates"`
		Rate         float64  `json:"inflation_rate"`
		RelinkedRate float64  `json:"relinked_inflation_rate"`
	}
}

func principalDate(t *testing.T, value string) Date {
	t.Helper()
	parts := strings.Split(value, "-")
	if len(parts) != 3 {
		t.Fatal("invalid oracle date", value)
	}
	values := [3]int{}
	for i := range values {
		n, err := strconv.Atoi(parts[i])
		if err != nil {
			t.Fatal(err)
		}
		values[i] = n
	}
	d, err := NewDate(values[2], values[1], values[0])
	if err != nil {
		t.Fatal(err)
	}
	return d
}
func principalNumber(t *testing.T, row map[string]string, name string) float64 {
	t.Helper()
	n, err := strconv.ParseFloat(row[name], 64)
	if err != nil || math.IsNaN(n) || math.IsInf(n, 0) {
		t.Fatal("invalid native fixture number", name, row[name], err)
	}
	return n
}
func principalRows(t *testing.T) ([]map[string]string, principalInputs) {
	t.Helper()
	data, err := os.ReadFile("testdata/inflation-principal-inputs.json")
	if err != nil {
		t.Fatal(err)
	}
	var inputs principalInputs
	if err = json.Unmarshal(data, &inputs); err != nil {
		t.Fatal(err)
	}
	data, err = os.ReadFile("testdata/inflation-principal-native.tsv")
	if err != nil {
		t.Fatal(err)
	}
	reader := csv.NewReader(strings.NewReader(string(data)))
	reader.Comma = '\t'
	records, err := reader.ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	if len(records) != 49 {
		t.Fatalf("native fixture row count %d want 49", len(records))
	}
	rows := []map[string]string{}
	for _, record := range records[1:] {
		row := map[string]string{}
		for i, name := range records[0] {
			row[name] = record[i]
		}
		rows = append(rows, row)
	}
	return rows, inputs
}
func principalMarket(t *testing.T, inputs principalInputs) (*Session, *Settings, *ZeroInflationIndex, *DayCounter) {
	t.Helper()
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	settings, err := s.NewSettings()
	if err != nil {
		t.Fatal(err)
	}
	if err = settings.SetEvaluationDate(principalDate(t, inputs.Inputs.EvaluationDate)); err != nil {
		t.Fatal(err)
	}
	index, err := s.NewUKRPI(settings)
	if err != nil {
		t.Fatal(err)
	}
	for _, fixing := range inputs.Inputs.Fixings {
		if err = index.AddFixing(principalDate(t, fixing.Date), fixing.Value); err != nil {
			t.Fatal(err)
		}
	}
	dc, err := s.Actual365Fixed()
	if err != nil {
		t.Fatal(err)
	}
	curve := principalCurve(t, s, dc, inputs, inputs.Inputs.Rate)
	if err = index.LinkTo(curve); err != nil {
		t.Fatal(err)
	}
	curve.Close()
	return s, settings, index, dc
}
func principalCurve(t *testing.T, s *Session, dc *DayCounter, inputs principalInputs, rate float64) *ZeroInflationTermStructure {
	t.Helper()
	dates := []Date{}
	for _, iso := range inputs.Inputs.CurveDates {
		dates = append(dates, principalDate(t, iso))
	}
	curve, err := s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: principalDate(t, inputs.Inputs.EvaluationDate), BaseDate: dates[0], Frequency: Monthly, DayCounter: dc}, dates, []float64{rate, rate})
	if err != nil {
		t.Fatal(err)
	}
	return curve
}
func principalFromRow(t *testing.T, s *Session, index *ZeroInflationIndex, row map[string]string) *InflationCashFlow {
	t.Helper()
	kind := CpiFlat
	if row["interpolation"] == "linear" {
		kind = CpiLinear
	}
	lag := Period{int32(principalNumber(t, row, "lag_months")), Months}
	growth := row["growth_only"] == "true"
	var flow *InflationCashFlow
	var err error
	if row["kind"] == "explicit" {
		flow, err = s.NewCPICashFlow(CPICashFlowConfig{Index: index, Nominal: principalNumber(t, row, "nominal"), BaseCPI: principalNumber(t, row, "base_cpi"), ObservationDate: principalDate(t, row["observation_date"]), PaymentDate: principalDate(t, row["payment_date"]), ObservationLag: lag, Interpolation: kind, GrowthOnly: growth})
	} else {
		flow, err = s.NewZeroInflationCashFlow(ZeroInflationCashFlowConfig{Index: index, Nominal: principalNumber(t, row, "nominal"), StartDate: principalDate(t, row["start_date"]), EndDate: principalDate(t, row["observation_date"]), PaymentDate: principalDate(t, row["payment_date"]), ObservationLag: lag, Interpolation: kind, GrowthOnly: growth})
	}
	if err != nil {
		t.Fatal(err)
	}
	return flow
}

func TestInflationPrincipalNativeValuesConversionsAndLiveRelinking(t *testing.T) {
	rows, inputs := principalRows(t)
	for _, row := range rows {
		t.Run(row["id"], func(t *testing.T) {
			s, settings, index, dc := principalMarket(t, inputs)
			flow := principalFromRow(t, s, index, row)
			generic, err := flow.AsCashFlow()
			if err != nil {
				t.Fatal(err)
			}
			leg, err := flow.AsLeg()
			if err != nil {
				t.Fatal(err)
			}
			for _, phase := range []string{"original", "relinked_unmodified"} {
				if phase == "relinked_unmodified" {
					curve := principalCurve(t, s, dc, inputs, inputs.Inputs.RelinkedRate)
					if err = index.LinkTo(curve); err != nil {
						t.Fatal(err)
					}
					curve.Close()
				}
				queries := map[string]func() (float64, error){"amount": flow.Amount, "base_fixing": flow.BaseFixing, "index_fixing": flow.IndexFixing}
				for name, query := range queries {
					got, e := query()
					tolerance := 1e-11
					if name == "amount" {
						tolerance = 1e-9
					}
					want := principalNumber(t, row, phase+"_"+name)
					if e != nil || !cpiNear(got, want, tolerance) {
						t.Fatalf("%s %s %.17g want %.17g: %v", phase, name, got, want, e)
					}
				}
			}
			got, err := flow.Amount()
			if err != nil {
				t.Fatal(err)
			}
			pay, err := flow.Date()
			if err != nil || pay != principalDate(t, row["payment_date"]) {
				t.Fatal("payment date", pay, err)
			}
			gp, err := generic.Date()
			if err != nil || gp != pay {
				t.Fatal("generic date", err)
			}
			discount, err := s.NewFlatForward(principalDate(t, inputs.Inputs.EvaluationDate), .035, dc)
			if err != nil {
				t.Fatal(err)
			}
			npv, err := leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: settings})
			if err != nil {
				t.Fatal(err)
			}
			want := 0.0
			days := pay.DaysSince(principalDate(t, inputs.Inputs.EvaluationDate))
			if days > 0 {
				want = got * math.Exp(-.035*float64(days)/365)
			}
			if !cpiNear(npv, want, 1e-8) {
				t.Fatalf("NPV %.17g want %.17g", npv, want)
			}
			other, err := s.NewSettings()
			if err != nil {
				t.Fatal(err)
			}
			if err = other.SetEvaluationDate(principalDate(t, inputs.Inputs.EvaluationDate)); err != nil {
				t.Fatal(err)
			}
			if _, err = leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: other}); err == nil {
				t.Fatal("different Settings accepted")
			}
			if err = flow.Close(); err != nil {
				t.Fatal(err)
			}
			index.Close()
			settings.Close()
			if _, err = flow.Amount(); err == nil {
				t.Fatal("closed principal accepted")
			}
			v, err := generic.Amount()
			if err != nil || !cpiNear(v, got, 1e-9) {
				t.Fatal("generic retained graph", v, err)
			}
			item, err := leg.At(0)
			if err != nil {
				t.Fatal(err)
			}
			v, err = item.Amount()
			if err != nil || !cpiNear(v, got, 1e-9) {
				t.Fatal("leg retained graph", v, err)
			}
		})
	}
}

func TestInflationPrincipalDomainsSessionsNilAndConcurrency(t *testing.T) {
	rows, inputs := principalRows(t)
	s, _, index, _ := principalMarket(t, inputs)
	base := CPICashFlowConfig{Index: index, Nominal: 100, BaseCPI: 200, ObservationDate: principalDate(t, "2024-07-15"), PaymentDate: principalDate(t, "2026-07-15"), ObservationLag: Period{3, Months}, Interpolation: CpiFlat, GrowthOnly: true}
	for _, bad := range []float64{0, 1e-20, math.NaN(), math.Inf(1)} {
		a := base
		a.BaseCPI = bad
		if _, err := s.NewCPICashFlow(a); err == nil {
			t.Fatal("invalid explicit base accepted", bad)
		}
	}
	for _, lag := range []Period{{-1, Months}, {math.MaxInt32, Months}, {math.MinInt32, Months}, {3, TimeUnit(99)}} {
		a := base
		a.ObservationLag = lag
		if _, err := s.NewCPICashFlow(a); err == nil {
			t.Fatal("invalid lag accepted", lag)
		}
	}
	bad := base
	bad.Interpolation = CpiInterpolationType(99)
	if _, err := s.NewCPICashFlow(bad); err == nil {
		t.Fatal("invalid interpolation accepted")
	}
	bad = base
	bad.PaymentDate = Date{}
	if _, err := s.NewCPICashFlow(bad); err == nil {
		t.Fatal("null date accepted")
	}
	s2, _, index2, _ := principalMarket(t, inputs)
	bad = base
	bad.Index = index2
	if _, err := s.NewCPICashFlow(bad); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("mixed session", err)
	}
	_ = s2
	var nilFlow *InflationCashFlow
	if _, err := nilFlow.Amount(); err == nil {
		t.Fatal("nil amount accepted")
	}
	if _, err := nilFlow.Date(); err == nil {
		t.Fatal("nil date accepted")
	}
	if _, err := nilFlow.AsLeg(); err == nil {
		t.Fatal("nil leg accepted")
	}
	if _, err := nilFlow.AsCashFlow(); err == nil {
		t.Fatal("nil flow accepted")
	}
	flow := principalFromRow(t, s, index, rows[0])
	want, err := flow.Amount()
	if err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	for range 12 {
		wg.Go(func() {
			for range 8 {
				got, e := flow.Amount()
				if e != nil || !cpiNear(got, want, 1e-9) {
					t.Errorf("concurrent query %g: %v", got, e)
				}
			}
		})
	}
	wg.Wait()
}

func TestInflationPrincipalLazyAndReadTimeGuards(t *testing.T) {
	_, inputs := principalRows(t)
	s, settings, index, _ := principalMarket(t, inputs)
	a := ZeroInflationCashFlowConfig{Index: index, Nominal: 100, StartDate: principalDate(t, "2024-01-15"), EndDate: principalDate(t, "2199-12-15"), PaymentDate: principalDate(t, "2199-12-31"), ObservationLag: Period{3, Months}, Interpolation: CpiLinear, GrowthOnly: true}
	flow, err := s.NewZeroInflationCashFlow(a)
	if err != nil {
		t.Fatal(err)
	}
	generic, err := flow.AsCashFlow()
	if err != nil {
		t.Fatal(err)
	}
	if _, err = generic.Amount(); err == nil {
		t.Fatal("Linear date overflow accepted")
	}
	for _, lag := range []Period{{-1, Months}, {math.MaxInt32, Years}, {math.MinInt32, Months}, {0, TimeUnit(99)}} {
		a.ObservationLag = lag
		if _, err = s.NewZeroInflationCashFlow(a); err == nil {
			t.Fatal("live lag accepted", lag)
		}
	}
	flow = principalFromRow(t, s, index, map[string]string{"kind": "explicit", "nominal": "100", "base_cpi": "200", "observation_date": "2024-07-15", "payment_date": "2026-07-15", "lag_months": "3", "interpolation": "flat", "growth_only": "true"})
	generic, err = flow.AsCashFlow()
	if err != nil {
		t.Fatal(err)
	}
	if err = settings.SetEvaluationDate(principalDate(t, "1901-01-01")); err != nil {
		t.Fatal(err)
	}
	if _, err = generic.Amount(); err == nil {
		t.Fatal("evaluation date underflow accepted")
	}
}

func TestInflationPrincipalLazyMissingAndFiniteGenericFlow(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	settings, err := s.NewSettings()
	if err != nil {
		t.Fatal(err)
	}
	if err = settings.SetEvaluationDate(principalDate(t, "2025-08-18")); err != nil {
		t.Fatal(err)
	}
	index, err := s.NewUKRPI(settings)
	if err != nil {
		t.Fatal(err)
	}
	a := ZeroInflationCashFlowConfig{Index: index, Nominal: 100, StartDate: principalDate(t, "2024-01-15"), EndDate: principalDate(t, "2024-07-15"), PaymentDate: principalDate(t, "2026-07-15"), ObservationLag: Period{0, Months}, Interpolation: CpiFlat, GrowthOnly: true}
	flow, err := s.NewZeroInflationCashFlow(a)
	if err != nil {
		t.Fatal(err)
	}
	generic, err := flow.AsCashFlow()
	if err != nil {
		t.Fatal(err)
	}
	leg, err := flow.AsLeg()
	if err != nil {
		t.Fatal(err)
	}
	item, err := leg.At(0)
	if err != nil {
		t.Fatal(err)
	}
	for _, query := range []func() (float64, error){flow.Amount, generic.Amount, item.Amount} {
		if _, err = query(); err == nil {
			t.Fatal("missing fixing accepted")
		}
	}
	if err = index.AddFixing(principalDate(t, "2024-01-01"), 0); err != nil {
		t.Fatal(err)
	}
	if err = index.AddFixing(principalDate(t, "2024-07-01"), 110); err != nil {
		t.Fatal(err)
	}
	for _, query := range []func() (float64, error){flow.Amount, generic.Amount, item.Amount} {
		if _, err = query(); err == nil {
			t.Fatal("non-finite zero-base amount accepted")
		}
	}
	if base, err := flow.BaseFixing(); err != nil || base != 0 {
		t.Fatal("zero base getter", base, err)
	}
}

func TestInflationPrincipalRejectsOverflowBaseThroughEveryConversion(t *testing.T) {
	_, inputs := principalRows(t)
	s, settings, index, dc := principalMarket(t, inputs)
	curve := principalCurve(t, s, dc, inputs, 1e308)
	if err := index.LinkTo(curve); err != nil {
		t.Fatal(err)
	}
	flow, err := s.NewZeroInflationCashFlow(ZeroInflationCashFlowConfig{Index: index, Nominal: 100, StartDate: principalDate(t, "2026-01-15"), EndDate: principalDate(t, "2024-07-15"), PaymentDate: principalDate(t, "2026-07-15"), ObservationLag: Period{0, Months}, Interpolation: CpiFlat, GrowthOnly: true})
	if err != nil {
		t.Fatal(err)
	}
	generic, err := flow.AsCashFlow()
	if err != nil {
		t.Fatal(err)
	}
	leg, err := flow.AsLeg()
	if err != nil {
		t.Fatal(err)
	}
	item, err := leg.At(0)
	if err != nil {
		t.Fatal(err)
	}
	discount, err := s.NewFlatForward(principalDate(t, inputs.Inputs.EvaluationDate), 0, dc)
	if err != nil {
		t.Fatal(err)
	}
	for _, query := range []func() (float64, error){flow.BaseFixing, flow.Amount, generic.Amount, item.Amount, func() (float64, error) { return leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: settings}) }} {
		if _, err = query(); err == nil {
			t.Fatal("overflow base hidden by a finite final ratio")
		}
	}
	if level, err := flow.IndexFixing(); err != nil || math.IsInf(level, 0) || math.IsNaN(level) {
		t.Fatal("finite numerator", level, err)
	}
}
