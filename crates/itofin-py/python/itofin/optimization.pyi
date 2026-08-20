# Hand-written stubs for itofin.optimization; sync manually with src/calibration.rs (#517).

class LevenbergMarquardt:
    """The least-squares optimizer used to fit model parameters.

    Wraps the MINPACK lmdif routine. The Jacobian comes from a built-in
    forward-difference scheme by default; the cost function's own jacobian
    method is used instead when use_cost_functions_jacobian is set.
    """

    def __init__(
        self,
        epsfcn: float = 1e-8,
        xtol: float = 1e-8,
        gtol: float = 1e-8,
        use_cost_functions_jacobian: bool = False,
    ) -> None:
        """Initialize the optimizer; the defaults are QuantLib's.

        Args:
            epsfcn: The finite-difference step seed used when the Jacobian is
                computed by differences.
            xtol: The tolerance on the independent variable.
            gtol: The tolerance on the gradient.
            use_cost_functions_jacobian: Use the cost function's own jacobian
                method (a central difference, order 2 but costlier) instead of
                the built-in forward-difference scheme.
        """
        ...

class EndCriteria:
    """The optimizer stopping rule.

    Carries the iteration cap and the stationarity thresholds an optimization
    run is tested against.
    """

    def __init__(
        self,
        max_iterations: int,
        max_stationary_state_iterations: int | None,
        root_epsilon: float,
        function_epsilon: float,
        gradient_norm_epsilon: float | None,
    ) -> None:
        """Initialize the criteria.

        Args:
            max_iterations: The iteration count at which the run stops.
            max_stationary_state_iterations: How many consecutive stationary
                iterations are tolerated before the run is called converged;
                None defaults to min(max_iterations / 2, 100).
            root_epsilon: The variation of the independent variable below which
                an iteration counts as stationary.
            function_epsilon: The variation of the function value below which an
                iteration counts as stationary, and, for a cost function known
                to be positive, the value below which the run has converged.
            gradient_norm_epsilon: The gradient norm below which the run has
                converged; None defaults to function_epsilon.

        Raises:
            ItofinError: Unless 1 < max_stationary_state_iterations <
                max_iterations, or if any epsilon is negative or non-finite.
        """
        ...
