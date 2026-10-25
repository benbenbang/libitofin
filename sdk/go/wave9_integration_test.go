package itofin

import (
	"math"
	"testing"
)

func wave9Near(value, expected float64) bool {
	return !math.IsNaN(value) && !math.IsInf(value, 0) && math.Abs(value-expected) <= 1e-10
}

func wave9Check(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

func wave9Date(t *testing.T, day, month, year int) Date {
	t.Helper()
	date, err := NewDate(day, month, year)
	wave9Check(t, err)
	return date
}

func wave9Market(t *testing.T, today Date) (*Session, *Settings, *ZeroInflationIndex) {
	t.Helper()
	s, err := NewSession()
	wave9Check(t, err)
	t.Cleanup(func() { wave9Check(t, s.Close()) })
	settings, err := s.NewSettings()
	wave9Check(t, err)
	wave9Check(t, settings.SetEvaluationDate(today))
	index, err := s.NewUKRPI(settings)
	wave9Check(t, err)
	return s, settings, index
}

func TestWave9PrincipalInspectionGuardsAndRetention(t *testing.T) {
	for _, live := range []bool{false, true} {
		t.Run(map[bool]string{false: "explicit", true: "live"}[live], func(t *testing.T) {
			today, payment := wave9Date(t, 1, 1, 2025), wave9Date(t, 15, 7, 2026)
			s, settings, index := wave9Market(t, today)
			var flow *InflationCashFlow
			var err error
			if live {
				flow, err = s.NewZeroInflationCashFlow(ZeroInflationCashFlowConfig{Index: index, Nominal: 1000, StartDate: wave9Date(t, 15, 1, 2024), EndDate: wave9Date(t, 15, 7, 2024), PaymentDate: payment, ObservationLag: Period{0, Months}, Interpolation: CpiFlat, GrowthOnly: true})
			} else {
				flow, err = s.NewCPICashFlow(CPICashFlowConfig{Index: index, Nominal: 1000, BaseCPI: 100, ObservationDate: wave9Date(t, 15, 7, 2024), PaymentDate: payment, ObservationLag: Period{0, Months}, Interpolation: CpiFlat, GrowthOnly: true})
			}
			wave9Check(t, err)
			generic, err := flow.AsCashFlow()
			wave9Check(t, err)
			leg, err := flow.AsLeg()
			wave9Check(t, err)
			query := CashFlowsPaymentConfig{Settings: settings}
			start, err := leg.StartDate()
			wave9Check(t, err)
			end, err := leg.MaturityDate()
			wave9Check(t, err)
			if start != payment || end != payment {
				t.Fatal("one-flow structural span changed")
			}
			next, err := leg.NextCashFlowDate(query)
			if err != nil || next == nil || *next != payment {
				t.Fatal("date inspection resolved missing fixings", next, err)
			}
			previous, err := leg.PreviousCashFlowAmount(query)
			if err != nil || previous != nil {
				t.Fatal("missing previous flow", previous, err)
			}
			if _, err = leg.NextCashFlowAmount(query); err == nil {
				t.Fatal("missing principal fixing accepted")
			}
			wave9Check(t, index.AddFixing(wave9Date(t, 1, 1, 2024), 100))
			wave9Check(t, index.AddFixing(wave9Date(t, 1, 7, 2024), 120))
			other, err := s.NewSettings()
			wave9Check(t, err)
			wave9Check(t, other.SetEvaluationDate(today))
			if _, err = leg.NextCashFlowDate(CashFlowsPaymentConfig{Settings: other}); err == nil {
				t.Fatal("equal-date foreign Settings accepted")
			}
			wave9Check(t, settings.SetEvaluationDate(wave9Date(t, 1, 1, 1901)))
			next, err = leg.NextCashFlowDate(query)
			if err != nil || next == nil || *next != payment {
				t.Fatal("date-only query evaluated CPI", next, err)
			}
			if _, err = leg.NextCashFlowAmount(query); err == nil {
				t.Fatal("read-time CPI date underflow accepted")
			}
			wave9Check(t, settings.SetEvaluationDate(today))
			wave9Check(t, flow.Close())
			wave9Check(t, index.Close())
			amount, err := leg.NextCashFlowAmount(query)
			if err != nil || amount == nil || !wave9Near(*amount, 200) {
				t.Fatal("retained principal inspection", amount, err)
			}
			dc, err := s.Actual365Fixed()
			wave9Check(t, err)
			discount, err := s.NewFlatForward(today, 0, dc)
			wave9Check(t, err)
			value, err := leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: settings})
			if err != nil || !wave9Near(value, 200) {
				t.Fatal("inspected principal NPV", value, err)
			}
			if _, err = leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: other}); err == nil {
				t.Fatal("equal-date foreign NPV Settings accepted")
			}
			after := wave9Date(t, 16, 7, 2026)
			next, err = leg.NextCashFlowDate(CashFlowsPaymentConfig{Settings: settings, SettlementDate: &after})
			if err != nil || next != nil {
				t.Fatal("missing next payment", next, err)
			}
			wave9Check(t, settings.Close())
			value, err = generic.Amount()
			if err != nil || !wave9Near(value, 200) {
				t.Fatal("erased principal lost retained Settings", value, err)
			}
		})
	}
}

func TestWave9BaseDateLegInspectionLazinessAndAggregation(t *testing.T) {
	settlement, payment := wave9Date(t, 1, 1, 2024), wave9Date(t, 31, 7, 2024)
	s, settings, index := wave9Market(t, settlement)
	dc, err := s.Actual365Fixed()
	wave9Check(t, err)
	calendar, err := s.NullCalendar()
	wave9Check(t, err)
	schedule, err := s.NewSchedule(ScheduleConfig{Start: wave9Date(t, 31, 1, 2024), End: payment, Frequency: Semiannual, Calendar: calendar, Convention: Unadjusted})
	wave9Check(t, err)
	builder, err := s.NewCPILegFromBaseDate(CPILegConfig{Schedule: schedule, Index: index, ObservationLag: Period{1, Months}, Interpolation: CpiFlat, Notionals: []float64{1000}, FixedRates: []float64{0}, DayCounter: dc, PaymentCalendar: calendar, PaymentAdjustment: Unadjusted, GrowthOnly: true}, nil)
	wave9Check(t, err)
	leg, err := builder.Build()
	wave9Check(t, err)
	coupon, err := leg.At(0)
	wave9Check(t, err)
	zero, err := coupon.Amount()
	if err != nil || zero != 0 {
		t.Fatal("zero-rate coupon queried missing CPI", zero, err)
	}
	query := CashFlowsPaymentConfig{Settings: settings, SettlementDate: &settlement}
	next, err := leg.NextCashFlowDate(query)
	if err != nil || next == nil || *next != payment {
		t.Fatal("lazy principal payment date", next, err)
	}
	if _, err = leg.NextCashFlowAmount(query); err == nil {
		t.Fatal("same-date aggregation skipped missing principal")
	}
	wave9Check(t, settings.SetEvaluationDate(wave9Date(t, 1, 1, 2025)))
	wave9Check(t, index.AddFixing(wave9Date(t, 1, 12, 2023), 100))
	wave9Check(t, index.AddFixing(wave9Date(t, 1, 6, 2024), 120))
	wave9Check(t, builder.Close())
	wave9Check(t, index.Close())
	wave9Check(t, schedule.Close())
	wave9Check(t, calendar.Close())
	amount, err := leg.NextCashFlowAmount(query)
	if err != nil || amount == nil || !wave9Near(*amount, 200) {
		t.Fatal("same-date retained principal aggregate", amount, err)
	}
	include := false
	query.SettlementDate, query.IncludeSettlementDateFlows = &payment, &include
	amount, err = leg.NextCashFlowAmount(query)
	if err != nil || amount != nil {
		t.Fatal("excluded settlement payment", amount, err)
	}
	amount, err = leg.PreviousCashFlowAmount(query)
	if err != nil || amount == nil || !wave9Near(*amount, 200) {
		t.Fatal("previous aggregate omitted principal", amount, err)
	}
	include = true
	amount, err = leg.NextCashFlowAmount(query)
	if err != nil || amount == nil || !wave9Near(*amount, 200) {
		t.Fatal("included settlement aggregate", amount, err)
	}
}
