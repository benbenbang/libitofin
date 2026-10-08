"""Find a negative scalar minimum with seeded, bounded firefly search."""

# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.models import CalibrationErrorType, HullWhite, SwaptionHelper
from itofin.optimization import EndCriteria, Firefly
from itofin.optimize import minimize
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter, Period


def objective(x) -> float:
    """Return a signed scalar, not a residual or its absolute value."""
    return (x[0] - 1.25) ** 2 + 4.0 * (x[1] + 0.75) ** 2 - 3.0


def calibrate_hull_white() -> None:
    """Fit one volatility parameter while keeping mean reversion fixed."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 2, 2002))
    curve = FlatForward(Date(19, 2, 2002), 0.04875825, DayCounter.actual365_fixed())
    index = Euribor.six_months(curve, settings)
    model = HullWhite(curve, 0.05, 0.01)
    helper = SwaptionHelper(
        Period(1, "Years"),
        Period(5, "Years"),
        0.1148,
        index,
        Period(1, "Years"),
        DayCounter.thirty360_bond_basis(),
        DayCounter.actual360(),
        curve,
        CalibrationErrorType.RelativePriceError,
        1.0,
    )
    method = Firefly(
        [(0.001, 0.03)],
        seed=42,
        population_size=8,
        xatol=1e-7,
        fatol=1e-10,
        maxiter=1000,
        maxfev=100000,
    )
    criteria = EndCriteria(1000, 50, 1e-6, 1e-8, 1e-8)
    model.calibrate([helper], method, criteria, True)
    result = method.last_result()
    assert result is not None
    assert result.success, result.message
    assert model.a() == 0.05
    assert 0.001 <= model.sigma() <= 0.03
    assert result.fun < 1e-6
    print(f"fixed a={model.a()}, calibrated sigma={model.sigma():.10f}")
    print(f"calibration cost={result.fun:.10g}, solver calls={result.nfev}")


def main() -> None:
    """Check the analytic optimum and report actual evaluation counts."""
    calls = 0

    def counted_objective(x) -> float:
        nonlocal calls
        calls += 1
        return objective(x)

    result = minimize(
        counted_objective,
        [-3.0, 3.0],
        method="Firefly",
        bounds=[(-4.0, 4.0), (-4.0, 4.0)],
        options={
            "seed": 42,
            "population_size": 24,
            "xatol": 1e-7,
            "fatol": 1e-10,
        },
    )
    assert result.success, result.message
    assert abs(result.x[0] - 1.25) < 1e-5
    assert abs(result.x[1] + 0.75) < 1e-5
    assert abs(result.fun + 3.0) < 1e-8
    assert result.nfev == calls
    assert result.njev == 0
    print(f"x={result.x}, scalar minimum={result.fun:.10f}")
    print(f"generations={result.nit}, objective calls={result.nfev}")
    print(result.status, result.message)
    calibrate_hull_white()


if __name__ == "__main__":
    main()
