# Joint yield curves

`JointYieldCurves` fits two mutually coupled curves in one solve. Both members use
Discount traits, LogLinear interpolation and GlobalBootstrap. Member 0 forecasts
the base Ibor index; member 1 forecasts the other index. This API requires
v0.27.0 or newer.

Prepare two plain helper lists and `IborIborBasisSwapRateHelper` templates. Each
template retains its live quote, tenor, calendar/convention, settlement lag,
end-of-month choice, both index prototypes and exogenous discount curve.
`bootstrap_base_curve=True` contributes to member 0; `False` contributes to member
1. Both sides are required, with the same index objects and settings throughout.
The assembler copies basis templates onto private forecast links; their original
quote/date inspectors remain usable, but they are not rebound to the joint curves.

=== "Python"

    ```python
    joint = JointYieldCurves(
        reference_date, fra_helpers, swap_helpers, basis_templates, day_counter,
    )
    curve3m, curve6m = joint.curve(0), joint.curve(1)
    del joint
    print(curve3m.discount(5.0))
    ```

=== "Go"

    ```go
    joint, err := session.NewJointYieldCurves(itofin.JointYieldCurvesConfig{
        ReferenceDate: reference, FirstHelpers: fraHelpers,
        SecondHelpers: swapHelpers, BasisHelpers: basisTemplates,
        DayCounter: dayCounter, Accuracy: 1e-10,
    })
    if err != nil { return err }
    defer joint.Close()
    curve3m, err := joint.Curve(0)
    if err != nil { return err }
    defer curve3m.Close()
    ```

Every exported curve retains both contributors. Indices and instruments built
from these curves can outlive the assembler and input wrappers. Plain helper
lists must contain distinct helpers with no live curve owner; reserve them for
this assembly. Solver failures remain ordinary errors on curve queries.
`FlatForward.from_quote` / `NewFlatForwardFromQuote` supplies a live discount
curve. `IborIndex.add_fixing` / `AddFixing` notifies retained and cloned indices;
conflicting stored fixings are rejected.

The complete [Python fixture](https://github.com/benbenbang/libitofin/blob/main/crates/itofin-py/tests/test_joint_yield_curves.py)
and [Go fixture](https://github.com/benbenbang/libitofin/blob/main/sdk/go/joint_curves_test.go)
reproduce QuantLib's `testMultiCurveTwoPiecewiseYieldCurves`: independent FRAs
use `1e-12` relative tolerance and independent swap NPVs use `1e-10` absolute
tolerance. Fixing-update cases retain future coupons; a fully fixed short basis
instrument cannot independently constrain a new forecast node.
