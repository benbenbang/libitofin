#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>

int main(void) {
    ItofinError error = {0, {0}};
    ItofinContext *ctx = 0;
    uint64_t settings=0,cal=0,dc=0,curve=0,model=0,engine=0,bond=0;
    int32_t reference=0,issue=0,maturity=0;
    assert(itofin_context_new(&ctx,&error)==0);
    assert(itofin_date_new(12,10,2026,&reference,&error)==0);
    assert(itofin_date_new(7,7,2026,&issue,&error)==0);
    assert(itofin_date_new(7,7,2031,&maturity,&error)==0);
    assert(itofin_settings_new(ctx,&settings,&error)==0);
    assert(itofin_settings_set_evaluation_date(ctx,settings,reference,&error)==0);
    assert(itofin_calendar_new(ctx,1,&cal,&error)==0);
    assert(itofin_day_counter_new(ctx,1,&dc,&error)==0);
    assert(itofin_flat_forward_new(ctx,reference,0.03,dc,&curve,&error)==0);
    assert(itofin_hullwhite_new(ctx,curve,0.1,0.01,&model,&error)==0);
    assert(itofin_tree_callable_bond_engine_new(ctx,model,100,settings,&engine,&error)==0);
    struct ItofinBondConfig common={.settlement_days=0,.face_amount=1000,.redemption=100,.settings=settings,.issue_date=issue,.payment_convention=2,.calendar=cal};
    assert(itofin_callable_zero_coupon_bond_new(ctx,common,maturity,dc,0,0,&bond,&error)==0);
    struct ItofinCallableBondRiskConfig config={.tree_engine=engine,.day_counter=dc,.compounding=2,.frequency=0,.flags=0,.bump=NAN};
    double duration=-999,convexity=-999,explicit_bump=-999;
    assert(itofin_callable_bond_effective_duration(ctx,bond,0.0075,config,&duration,&error)==0);
    assert(itofin_callable_bond_effective_convexity(ctx,bond,0.0075,config,&convexity,&error)==0);
    assert(isfinite(duration)&&isfinite(convexity));
    config.flags=1;config.bump=2e-4;
    assert(itofin_callable_bond_effective_duration(ctx,bond,0.0075,config,&explicit_bump,&error)==0);
    assert(explicit_bump==duration);
    config.bump=0;explicit_bump=-999;
    assert(itofin_callable_bond_effective_duration(ctx,bond,0.0075,config,&explicit_bump,&error)==ITOFIN_INVALID_ARGUMENT);
    assert(explicit_bump==-999);
    config.flags=2;
    assert(itofin_callable_bond_effective_convexity(ctx,bond,0.0075,config,&explicit_bump,&error)==ITOFIN_INVALID_ARGUMENT);
    assert(explicit_bump==-999);
    config.flags=0;
    assert(itofin_callable_bond_effective_duration(ctx,bond,0.0075,config,0,&error)==ITOFIN_INVALID_ARGUMENT);
    assert(itofin_context_free(ctx,&error)==0);
    return 0;
}
