#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

static void near(double got,double want,double tolerance) { assert(isfinite(got)); assert(fabs(got-want)<=tolerance); }
int main(void) {
    ItofinContext *ctx=NULL; ItofinError error={0,{0}};
    uint64_t settings=0,cal=0,dc=0,schedule=0,curve=0,quote=0,engine=0,bond=0;
    int32_t start=0,maturity=0,exercise=0;
    assert(itofin_context_new(&ctx,&error)==0);
    assert(itofin_settings_new(ctx,&settings,&error)==0);
    assert(itofin_date_new(15,1,2026,&start,&error)==0);
    assert(itofin_date_new(15,1,2031,&maturity,&error)==0);
    assert(itofin_date_new(15,1,2028,&exercise,&error)==0);
    assert(itofin_settings_set_evaluation_date(ctx,settings,start,&error)==0);
    assert(itofin_calendar_new(ctx,1,&cal,&error)==0);
    assert(itofin_day_counter_new(ctx,1,&dc,&error)==0);
    ItofinScheduleConfig sc={start,maturity,1,cal,2,0,2};
    assert(itofin_schedule_new(ctx,sc,&schedule,&error)==0);
    assert(itofin_flat_forward_new(ctx,start,0.04,dc,&curve,&error)==0);
    assert(itofin_quote_new(ctx,0.35,&quote,&error)==0);
    uint64_t untouched=991;
    assert(itofin_black_callable_bond_engine_new(ctx,quote,curve,settings,NULL,&error)==ITOFIN_INVALID_ARGUMENT);
    assert(itofin_black_callable_bond_engine_new(ctx,curve,curve,settings,&untouched,&error)==ITOFIN_INVALID_HANDLE);
    assert(untouched==991);
    assert(itofin_black_callable_bond_engine_new(ctx,quote,curve,settings,&engine,&error)==0);
    ItofinBondConfig common={3,1000,100,settings,start,2,0};
    ItofinFixedBondConfig config={common,schedule,dc,0,0,0,0,2,0,0};
    ItofinCallability call={100,0,0,exercise}; const double coupons[1]={0.055};
    assert(itofin_callable_fixed_rate_bond_new(ctx,config,coupons,1,&call,1,&bond,&error)==0);
    ItofinCallableImpliedVolatilityConfig options={0,NAN,0,NAN,NAN};
    double result=-991;
    assert(itofin_callable_bond_implied_volatility(ctx,bond,102.05721503812158,0,curve,options,&result,&error)==0);
    near(result,0.35,2e-8);
    assert(itofin_callable_bond_set_black_engine(ctx,bond,engine,&error)==0);
    assert(itofin_bond_real(ctx,bond,0,0,&result,&error)==0); near(result,1020.6885811590882,1e-8);
    assert(itofin_bond_real(ctx,bond,1,0,&result,&error)==0); near(result,1021.0242051757366,1e-8);
    assert(itofin_bond_real(ctx,bond,2,0,&result,&error)==0); near(result,102.05721503812158,1e-9);
    assert(itofin_bond_real(ctx,bond,3,0,&result,&error)==0); near(result,102.10242051757365,1e-9);
    options.flags=16;result=-991;
    assert(itofin_callable_bond_implied_volatility(ctx,bond,100,0,curve,options,&result,&error)==ITOFIN_INVALID_ARGUMENT);
    assert(result==-991 && error.code==ITOFIN_INVALID_ARGUMENT && strlen(error.message)>0);
    options.flags=2;options.max_evaluations=1;
    assert(itofin_callable_bond_implied_volatility(ctx,bond,102.05721503812158,0,curve,options,&result,&error)==ITOFIN_CORE_ERROR);
    assert(result==-991);
    options.flags=0;
    assert(itofin_callable_bond_implied_volatility(ctx,bond,100,0,curve,options,NULL,&error)==ITOFIN_INVALID_ARGUMENT);
    uint64_t other=0,wrong=0,plain=0;
    assert(itofin_settings_new(ctx,&other,&error)==0);
    assert(itofin_black_callable_bond_engine_new(ctx,quote,curve,other,&wrong,&error)==0);
    assert(itofin_callable_bond_set_black_engine(ctx,bond,wrong,&error)==ITOFIN_INVALID_ARGUMENT);
    common.calendar=cal;
    assert(itofin_zero_coupon_bond_new(ctx,common,maturity,&plain,&error)==0);
    assert(itofin_callable_bond_set_black_engine(ctx,plain,engine,&error)==ITOFIN_INVALID_ARGUMENT);
    uint64_t zero=0;call.call_type=1;
    assert(itofin_callable_zero_coupon_bond_new(ctx,common,maturity,dc,&call,1,&zero,&error)==0);
    assert(itofin_callable_bond_set_black_engine(ctx,zero,engine,&error)==0);
    assert(itofin_bond_real(ctx,zero,0,0,&result,&error)==0); near(result,923.4809761494753,1e-8);
    assert(itofin_bond_real(ctx,zero,1,0,&result,&error)==0); near(result,923.7846362474081,1e-8);
    assert(itofin_bond_real(ctx,zero,2,0,&result,&error)==0); near(result,92.37846362474082,1e-9);
    assert(itofin_bond_real(ctx,zero,3,0,&result,&error)==0); near(result,92.37846362474082,1e-9);
    assert(itofin_handle_release(ctx,quote,&error)==0);
    assert(itofin_handle_release(ctx,curve,&error)==0);
    assert(itofin_handle_release(ctx,engine,&error)==0);
    assert(itofin_handle_release(ctx,settings,&error)==0);
    assert(itofin_bond_action(ctx,bond,0,&error)==0);
    assert(itofin_bond_action(ctx,zero,0,&error)==0);
    assert(itofin_context_free(ctx,&error)==0);
    return 0;
}
