package itofin

import (
 "math"
 "sync"
 "testing"
)
func creditMust[T any](t *testing.T,v T,e error)T{t.Helper();if e!=nil{t.Fatal(e)};return v}
func TestCreditCachedMidpointAndLifecycle(t *testing.T){
 s0,e:=NewSession();s:=creditMust(t,s0,e);defer s.Close()
 today0,e:=NewDate(9,6,2006);today:=creditMust(t,today0,e)
 issue0,e:=NewDate(9,6,2005);issue:=creditMust(t,issue0,e)
 maturity0,e:=NewDate(9,6,2015);maturity:=creditMust(t,maturity0,e)
 dc0,e:=s.Actual360();dc:=creditMust(t,dc0,e)
 cal0,e:=s.Target();cal:=creditMust(t,cal0,e)
 settings0,e:=s.NewSettings();settings:=creditMust(t,settings0,e)
 if e=settings.SetEvaluationDate(today);e!=nil{t.Fatal(e)}
 q0,e:=s.NewSimpleQuote(.01234);q:=creditMust(t,q0,e)
 h0,e:=s.NewFlatHazardRate(FlatHazardConfig{Quote:q,DayCounter:dc,Calendar:cal,Settings:settings});h:=creditMust(t,h0,e)
 d0,e:=s.NewFlatForward(today,.06,dc);d:=creditMust(t,d0,e)
 engine0,e:=s.NewMidPointCdsEngine(CdsEngineConfig{Probability:h,Discount:d,Settings:settings,Recovery:.4});engine:=creditMust(t,engine0,e)
 schedule0,e:=s.NewSchedule(ScheduleConfig{Start:issue,End:maturity,Frequency:Semiannual,Calendar:cal,Convention:ModifiedFollowing});schedule:=creditMust(t,schedule0,e)
 cfg:=DefaultCdsConfig();cfg.Side=ProtectionSeller;cfg.Notional=10000;cfg.Spread=.012;cfg.Schedule=schedule;cfg.PaymentConvention=ModifiedFollowing;cfg.DayCounter=dc;cfg.Settings=settings
 cds0,e:=s.NewCreditDefaultSwap(cfg);cds:=creditMust(t,cds0,e)
 if _,e=cds.NPV();e==nil{t.Fatal("missing engine accepted")}
 if e=cds.SetEngine(engine);e!=nil{t.Fatal(e)}
 v,e:=cds.NPV();if e!=nil||math.Abs(v-295.0153398)>1e-7{t.Fatalf("cached NPV %.12f: %v",v,e)}
 spread,e:=cds.FairSpread();if e!=nil||math.Abs(spread-.007517539081)>1e-7{t.Fatalf("fair spread %g: %v",spread,e)}
 if e=q.SetValue(.02);e!=nil{t.Fatal(e)}
 calc,e:=cds.IsCalculated();if e!=nil||calc{t.Fatalf("quote update did not invalidate: %v",e)}
 changed,e:=cds.NPV();if e!=nil||changed==v{t.Fatalf("quote update did not reprice: %v",e)}
 for _,close:=range []func()error{engine.Close,h.Close,d.Close,q.Close,dc.Close,schedule.Close}{if e=close();e!=nil{t.Fatal(e)}}
 retained,e:=cds.NPV();if e!=nil||retained!=changed{t.Fatalf("dependency lifetime: %g %v",retained,e)}
 var wg sync.WaitGroup
 for range 8{wg.Add(1);go func(){defer wg.Done();x,e:=cds.NPV();if e!=nil||x!=retained{t.Errorf("concurrent query: %g %v",x,e)}}()};wg.Wait()
 if e=cds.Close();e!=nil{t.Fatal(e)};if _,e=cds.NPV();e==nil{t.Fatal("released instrument accepted")}
}
func TestCreditCurveNodesAndSessionIsolation(t *testing.T){
 s0,e:=NewSession();s:=creditMust(t,s0,e);defer s.Close()
 other0,e:=NewSession();other:=creditMust(t,other0,e);defer other.Close()
 a0,e:=NewDate(1,1,2025);a:=creditMust(t,a0,e);b0,e:=NewDate(1,1,2026);b:=creditMust(t,b0,e)
 dc0,e:=s.Actual365Fixed();dc:=creditMust(t,dc0,e)
 if _,e=other.NewFlatHazardRate(FlatHazardConfig{ReferenceDate:a,Rate:.02,DayCounter:dc});e==nil{t.Fatal("foreign day counter accepted")}
 c0,e:=s.NewInterpolatedHazardRateCurve([]Date{a,b},[]float64{.01,.02},dc);c:=creditMust(t,c0,e)
 n,e:=c.Nodes();if e!=nil||len(n)!=2||n[1].Rate!=.02||n[1].Date.Serial()!=b.Serial(){t.Fatalf("nodes: %v %v",n,e)}
 survival,e:=c.SurvivalProbabilityDate(b,false);if e!=nil||math.Abs(survival-math.Exp(-.02))>1e-12{t.Fatalf("survival %g: %v",survival,e)}
 if _,e=c.HazardRate(math.NaN(),false);e==nil{t.Fatal("NaN accepted")}
 if _,e=s.NewInterpolatedHazardRateCurve([]Date{b,a},[]float64{.01,.02},dc);e==nil{t.Fatal("unsorted dates accepted")}
}
