package itofin

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"testing"
)

type creditCompletionValues struct {
	NPV, Coupon, Default, Rebate float64
	FairSpread                   float64 `json:"fair_spread"`
	FairUpfront                  float64 `json:"fair_upfront"`
	RebateDate                   string  `json:"rebate_date"`
	Fix, Forwards                string
}

type creditCompletionOracle struct {
	Quantlib          string
	Schedule          []string
	Builder, Explicit creditCompletionValues
	Coupons           []struct {
		Date   string
		Amount float64
	}
	ISDA []creditCompletionValues
}

func creditCompletionRead(t *testing.T) creditCompletionOracle {
	t.Helper()
	data := pricingMust(os.ReadFile("testdata/credit_completion_oracle.json"))
	var out creditCompletionOracle
	pricingOK(t, json.Unmarshal(data, &out))
	if out.Quantlib != "1.43" {
		t.Fatalf("unexpected oracle version %q", out.Quantlib)
	}
	return out
}

func creditCompletionDate(text string) Date {
	var year, month, day int
	if _, err := fmt.Sscanf(text, "%d-%d-%d", &year, &month, &day); err != nil {
		panic(err)
	}
	return pricingMust(NewDate(day, month, year))
}

type creditCompletionFixture struct {
	s           *Session
	today, end  Date
	settings    *Settings
	dc, curveDC *DayCounter
	discount    *YieldTermStructure
	probability *DefaultProbabilityTermStructure
	engine      *CdsEngine
	schedule    *Schedule
	config      CdsConfig
	maker       MakeCreditDefaultSwapConfig
}

func creditCompletionSetup(t *testing.T) creditCompletionFixture {
	t.Helper()
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	f := creditCompletionFixture{s: s, today: creditCompletionDate("2026-06-15"), end: creditCompletionDate("2029-06-20")}
	f.settings = pricingMust(s.NewSettings())
	pricingOK(t, f.settings.SetEvaluationDate(f.today))
	f.dc = pricingMust(s.Actual360())
	f.curveDC = pricingMust(s.Actual365Fixed())
	f.discount = pricingMust(s.NewFlatForward(f.today, .03, f.curveDC))
	f.probability = pricingMust(s.NewFlatHazardRate(FlatHazardConfig{ReferenceDate: f.today, Rate: .02, DayCounter: f.curveDC}))
	f.engine = pricingMust(s.NewMidPointCdsEngine(CdsEngineConfig{Probability: f.probability, Discount: f.discount, Settings: f.settings, Recovery: .4}))
	f.schedule = pricingMust(s.NewSchedule(ScheduleConfig{Start: f.today, End: f.end, Frequency: Quarterly, Calendar: pricingMust(s.WeekendsOnly()), Convention: Following, Rule: pricingPtr(CDS), TerminationConvention: pricingPtr(Unadjusted)}))
	f.config = CdsConfig{Side: ProtectionBuyer, Notional: 1e7, Spread: .01, Schedule: f.schedule, PaymentConvention: Following, DayCounter: f.dc, Settings: f.settings, ProtectionStart: &f.today}
	f.maker = MakeCreditDefaultSwapConfig{TermDate: f.end, RunningSpread: .01, Settings: f.settings, Nominal: pricingPtr(1e7), TradeDate: &f.today}
	return f
}

func creditCompletionCheck(t *testing.T, cds *CreditDefaultSwap, want creditCompletionValues) {
	t.Helper()
	pricingNear(t, pricingMust(cds.NPV()), want.NPV, 1e-8)
	pricingNear(t, pricingMust(cds.CouponLegNPV()), want.Coupon, 1e-8)
	pricingNear(t, pricingMust(cds.DefaultLegNPV()), want.Default, 1e-8)
	pricingNear(t, pricingMust(cds.FairSpread()), want.FairSpread, 1e-12)
	pricingNear(t, pricingMust(cds.FairUpfront()), want.FairUpfront, 1e-12)
	amount, date, err := cds.Rebate()
	pricingOK(t, err)
	if amount == nil || date == nil {
		t.Fatal("missing accrual rebate")
	}
	pricingNear(t, *amount, want.Rebate, 1e-8)
	if *date != creditCompletionDate(want.RebateDate) {
		t.Fatalf("rebate date %v", date)
	}
	queried := pricingMust(cds.AccrualRebateDate())
	if queried == nil || *queried != *date {
		t.Fatalf("rebate date query %v", queried)
	}
}

func TestCreditCompletionBuilderCashflowsAndFairUpfront(t *testing.T) {
	f := creditCompletionSetup(t)
	oracle := creditCompletionRead(t)
	built := pricingMust(f.s.MakeCreditDefaultSwap(f.maker))
	explicit := pricingMust(f.s.NewCreditDefaultSwap(f.config))
	for _, cds := range []*CreditDefaultSwap{built, explicit} {
		if pricingMust(cds.IsCalculated()) {
			t.Fatal("unpriced CDS marked calculated")
		}
		pricingOK(t, cds.SetEngine(f.engine))
		pricingOK(t, cds.Calculate())
		if !pricingMust(cds.IsCalculated()) {
			t.Fatal("calculation not cached")
		}
		pricingNear(t, pricingMust(cds.Notional()), 1e7, 0)
	}
	creditCompletionCheck(t, built, oracle.Builder)
	creditCompletionCheck(t, explicit, oracle.Explicit)
	dates := pricingMust(f.schedule.Dates())
	if len(dates) != len(oracle.Schedule) || len(oracle.Coupons)+1 != len(dates) {
		t.Fatal("schedule length")
	}
	for i, d := range dates {
		if d != creditCompletionDate(oracle.Schedule[i]) {
			t.Fatalf("schedule date %d: %v", i, d)
		}
	}
	var couponPV, defaultPV, explicitCouponPV float64
	exponential := func(d Date, rate float64) float64 {
		return math.Exp(-rate * float64(d.Serial()-f.today.Serial()) / 365)
	}
	for i, flow := range oracle.Coupons {
		start, end := dates[i], dates[i+1]
		payment := creditCompletionDate(flow.Date)
		days := float64(end.Serial() - start.Serial())
		extra := 0.
		if i == len(oracle.Coupons)-1 {
			extra = 1
		}
		amount := 1e7 * .01 * (days + extra) / 360
		pricingNear(t, amount, flow.Amount, 1e-8)
		if payment != end {
			t.Fatalf("unexpected payment convention at %d", i)
		}
		effectiveStart := start
		if i == 0 {
			effectiveStart = f.today
		}
		midpoint := pricingMust(effectiveStart.AddDays(int64(end.Serial()-effectiveStart.Serial()) / 2))
		probDefault := exponential(effectiveStart, .02) - exponential(end, .02)
		regularPV := exponential(payment, .05)
		accruedDays := float64(midpoint.Serial() - start.Serial())
		couponPV += amount*regularPV + probDefault*1e7*.01*(accruedDays+extra)/360*exponential(midpoint, .03)
		explicitCouponPV += 1e7*.01*days/360*regularPV + probDefault*1e7*.01*accruedDays/360*exponential(midpoint, .03)
		defaultPV += probDefault * 1e7 * .6 * exponential(midpoint, .03)
	}
	pricingNear(t, pricingMust(built.CouponLegNPV()), -couponPV, 1e-8)
	pricingNear(t, pricingMust(explicit.CouponLegNPV()), -explicitCouponPV, 1e-8)
	pricingNear(t, pricingMust(built.DefaultLegNPV()), defaultPV, 1e-8)
	pricingNear(t, pricingMust(explicit.NPV())-pricingMust(built.NPV()), couponPV-explicitCouponPV, 1e-8)
	lastStart := dates[len(dates)-2]
	lastMid := pricingMust(lastStart.AddDays(int64(f.end.Serial()-lastStart.Serial()) / 2))
	extraDayPV := 1e7 * .01 / 360 * (exponential(f.end, .05) + (exponential(lastStart, .02)-exponential(f.end, .02))*exponential(lastMid, .03))
	pricingNear(t, couponPV-explicitCouponPV, extraDayPV, 1e-8)
	pricingNear(t, pricingMust(built.Price(f.engine)), oracle.Builder.NPV, 1e-8)
	fair := pricingMust(built.FairUpfront())
	f.maker.UpfrontRate = &fair
	fairContract := pricingMust(f.s.MakeCreditDefaultSwap(f.maker))
	pricingNear(t, pricingMust(fairContract.Price(f.engine)), 0, 1e-8)
	pricingNear(t, pricingMust(fairContract.FairUpfront()), fair, 1e-12)
}

func TestCreditCompletionMidpointHazardAndProtectionSigns(t *testing.T) {
	f := creditCompletionSetup(t)
	buyer := pricingMust(f.s.NewCreditDefaultSwap(f.config))
	f.config.Side = ProtectionSeller
	seller := pricingMust(f.s.NewCreditDefaultSwap(f.config))
	buyerNPV := pricingMust(buyer.Price(f.engine))
	pricingNear(t, pricingMust(seller.Price(f.engine)), -buyerNPV, 1e-8)
	pricingNear(t, pricingMust(seller.CouponLegNPV()), -pricingMust(buyer.CouponLegNPV()), 1e-8)
	pricingNear(t, pricingMust(seller.DefaultLegNPV()), -pricingMust(buyer.DefaultLegNPV()), 1e-8)
	pricingNear(t, pricingMust(seller.FairSpread()), pricingMust(buyer.FairSpread()), 1e-12)
	pricingNear(t, pricingMust(seller.FairUpfront()), pricingMust(buyer.FairUpfront()), 1e-12)
	for _, cds := range []*CreditDefaultSwap{buyer, seller} {
		implied := pricingMust(cds.ImpliedHazardRate(pricingMust(cds.NPV()), f.discount, f.curveDC, .4, 1e-10, Midpoint))
		pricingNear(t, implied, .02, 1e-9)
	}
	isda := pricingMust(f.s.NewIsdaCdsEngine(CdsEngineConfig{Probability: f.probability, Discount: f.discount, Settings: f.settings, Recovery: .4}))
	isdaNPV := pricingMust(buyer.Price(isda))
	if math.Abs(isdaNPV-buyerNPV) < 1 {
		t.Fatal("fixture does not distinguish pricing-model dispatch")
	}
	pricingNear(t, pricingMust(seller.Price(isda)), -isdaNPV, 1e-8)
	pricingNear(t, pricingMust(buyer.ImpliedHazardRate(isdaNPV, f.discount, f.curveDC, .4, 1e-10, Isda)), .02, 1e-9)
}

func TestCreditCompletionRebateSettlementBusinessDays(t *testing.T) {
	f := creditCompletionSetup(t)
	for _, dates := range [][2]string{{"2026-06-15", "2026-06-18"}, {"2026-06-18", "2026-06-23"}, {"2026-06-19", "2026-06-24"}, {"2026-06-20", "2026-06-24"}} {
		t.Run(dates[0], func(t *testing.T) {
			trade := creditCompletionDate(dates[0])
			f.maker.TradeDate = &trade
			cds := pricingMust(f.s.MakeCreditDefaultSwap(f.maker))
			date := pricingMust(cds.AccrualRebateDate())
			if date == nil || *date != creditCompletionDate(dates[1]) {
				t.Fatalf("three business-day settlement: %v", date)
			}
		})
	}
	f.config.RebatesAccrual = pricingPtr(false)
	cds := pricingMust(f.s.NewCreditDefaultSwap(f.config))
	if date := pricingMust(cds.AccrualRebateDate()); date != nil {
		t.Fatalf("disabled rebate has settlement date %v", date)
	}
}
