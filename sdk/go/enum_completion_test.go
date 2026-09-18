package itofin

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func TestEnumPythonIntegerParity(t *testing.T) {
	data, err := os.ReadFile("testdata/enum_oracle.json")
	if err != nil {
		t.Fatal(err)
	}
	var oracle struct {
		Enums         int            `json:"enums"`
		PythonVersion string         `json:"python_version"`
		Values        map[string]int `json:"values"`
	}
	if err := json.Unmarshal(data, &oracle); err != nil {
		t.Fatal(err)
	}
	actual := map[string]int{
		"itofin.indexes.CpiInterpolationType.Flat":                        int(CpiFlat),
		"itofin.indexes.CpiInterpolationType.Linear":                      int(CpiLinear),
		"itofin.instruments.CapFloorType.Cap":                             int(CapType),
		"itofin.instruments.CapFloorType.Collar":                          int(CollarType),
		"itofin.instruments.CapFloorType.Floor":                           int(FloorType),
		"itofin.instruments.OptionType.Call":                              int(Call),
		"itofin.instruments.OptionType.Put":                               int(Put),
		"itofin.instruments.Position.Long":                                int(PositionLong),
		"itofin.instruments.Position.Short":                               int(PositionShort),
		"itofin.instruments.PricingModel.Isda":                            int(Isda),
		"itofin.instruments.PricingModel.Midpoint":                        int(Midpoint),
		"itofin.instruments.ProtectionSide.Buyer":                         int(ProtectionBuyer),
		"itofin.instruments.ProtectionSide.Seller":                        int(ProtectionSeller),
		"itofin.instruments.SettlementMethod.CollateralizedCashPrice":     int(CollateralizedCashPrice),
		"itofin.instruments.SettlementMethod.ParYieldCurve":               int(ParYieldCurve),
		"itofin.instruments.SettlementMethod.PhysicalCleared":             int(PhysicalCleared),
		"itofin.instruments.SettlementMethod.PhysicalOTC":                 int(PhysicalOTC),
		"itofin.instruments.SettlementType.Cash":                          int(SettlementCash),
		"itofin.instruments.SettlementType.Physical":                      int(SettlementPhysical),
		"itofin.instruments.SwapType.Payer":                               int(SwapPayer),
		"itofin.instruments.SwapType.Receiver":                            int(SwapReceiver),
		"itofin.models.CalibrationErrorType.ImpliedVolError":              int(ImpliedVolError),
		"itofin.models.CalibrationErrorType.PriceError":                   int(PriceError),
		"itofin.models.CalibrationErrorType.RelativePriceError":           int(RelativePriceError),
		"itofin.pricingengines.AccrualBias.HalfDayBias":                   int(HalfDayBias),
		"itofin.pricingengines.AccrualBias.NoBias":                        int(NoBias),
		"itofin.pricingengines.CashAnnuityModel.DiscountCurve":            int(AnnuityDiscountCurve),
		"itofin.pricingengines.CashAnnuityModel.SwapRate":                 int(AnnuitySwapRate),
		"itofin.pricingengines.ForwardsInCouponPeriod.Flat":               int(FlatForwards),
		"itofin.pricingengines.ForwardsInCouponPeriod.Piecewise":          int(PiecewiseForwards),
		"itofin.pricingengines.NumericalFix.NoFix":                        int(NoFix),
		"itofin.pricingengines.NumericalFix.Taylor":                       int(Taylor),
		"itofin.randomnumbers.DirectionIntegers.Jaeckel":                  int(Jaeckel),
		"itofin.randomnumbers.DirectionIntegers.JoeKuoD5":                 int(JoeKuoD5),
		"itofin.randomnumbers.DirectionIntegers.JoeKuoD6":                 int(JoeKuoD6),
		"itofin.randomnumbers.DirectionIntegers.JoeKuoD7":                 int(JoeKuoD7),
		"itofin.randomnumbers.DirectionIntegers.Kuo":                      int(Kuo),
		"itofin.randomnumbers.DirectionIntegers.Kuo2":                     int(Kuo2),
		"itofin.randomnumbers.DirectionIntegers.Kuo3":                     int(Kuo3),
		"itofin.randomnumbers.DirectionIntegers.SobolLevitan":             int(SobolLevitan),
		"itofin.randomnumbers.DirectionIntegers.SobolLevitanLemieux":      int(SobolLevitanLemieux),
		"itofin.randomnumbers.DirectionIntegers.Unit":                     int(Unit),
		"itofin.termstructures.BlackVolTimeExtrapolation.FlatVolatility":  int(FlatVolatility),
		"itofin.termstructures.BlackVolTimeExtrapolation.LinearVariance":  int(LinearVariance),
		"itofin.termstructures.BlackVolTimeExtrapolation.UseInterpolator": int(UseInterpolator),
		"itofin.termstructures.BondPriceType.Clean":                       int(Clean),
		"itofin.termstructures.BondPriceType.Dirty":                       int(Dirty),
		"itofin.termstructures.FuturesType.Asx":                           int(Asx),
		"itofin.termstructures.FuturesType.Custom":                        int(Custom),
		"itofin.termstructures.FuturesType.Imm":                           int(Imm),
		"itofin.termstructures.Pillar.LastRelevantDate":                   int(LastRelevantDate),
		"itofin.termstructures.Pillar.MaturityDate":                       int(MaturityDate),
		"itofin.termstructures.RateAveraging.Compound":                    int(CompoundAveraging),
		"itofin.termstructures.RateAveraging.Simple":                      int(SimpleAveraging),
		"itofin.termstructures.VolatilityType.Normal":                     int(Normal),
		"itofin.termstructures.VolatilityType.ShiftedLognormal":           int(ShiftedLognormal),
		"itofin.time.BusinessDayConvention.Following":                     int(Following),
		"itofin.time.BusinessDayConvention.HalfMonthModifiedFollowing":    int(HalfMonthModifiedFollowing),
		"itofin.time.BusinessDayConvention.ModifiedFollowing":             int(ModifiedFollowing),
		"itofin.time.BusinessDayConvention.ModifiedPreceding":             int(ModifiedPreceding),
		"itofin.time.BusinessDayConvention.Nearest":                       int(Nearest),
		"itofin.time.BusinessDayConvention.Preceding":                     int(Preceding),
		"itofin.time.BusinessDayConvention.Unadjusted":                    int(Unadjusted),
		"itofin.time.DateGeneration.Backward":                             int(Backward),
		"itofin.time.DateGeneration.CDS":                                  int(CDS),
		"itofin.time.DateGeneration.CDS2015":                              int(CDS2015),
		"itofin.time.DateGeneration.Forward":                              int(Forward),
		"itofin.time.DateGeneration.OldCDS":                               int(OldCDS),
		"itofin.time.DateGeneration.ThirdWednesday":                       int(ThirdWednesday),
		"itofin.time.DateGeneration.ThirdWednesdayInclusive":              int(ThirdWednesdayInclusive),
		"itofin.time.DateGeneration.Twentieth":                            int(Twentieth),
		"itofin.time.DateGeneration.TwentiethIMM":                         int(TwentiethIMM),
		"itofin.time.DateGeneration.Zero":                                 int(Zero),
		"itofin.time.Frequency.Annual":                                    int(Annual),
		"itofin.time.Frequency.Monthly":                                   int(Monthly),
		"itofin.time.Frequency.Quarterly":                                 int(Quarterly),
		"itofin.time.Frequency.Semiannual":                                int(Semiannual),
	}
	if oracle.PythonVersion != "0.22.0" || oracle.Enums != 24 || len(actual) != len(oracle.Values) {
		t.Fatalf("unexpected enum oracle provenance or size: version=%s enums=%d members=%d Go=%d",
			oracle.PythonVersion, oracle.Enums, len(oracle.Values), len(actual))
	}
	seen := make(map[string]bool)
	for name, want := range oracle.Values {
		seen[name[:strings.LastIndex(name, ".")]] = true
		t.Run(name, func(t *testing.T) {
			got, ok := actual[name]
			if !ok || got != want {
				t.Fatalf("integer conversion=%d present=%v; Python=%d", got, ok, want)
			}
		})
	}
	if len(seen) != oracle.Enums {
		t.Fatalf("fixture contains %d enum classes; expected %d", len(seen), oracle.Enums)
	}
}
