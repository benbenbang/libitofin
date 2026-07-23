# Hand-written stubs for itofin.optimization; sync manually with src/calibration.rs (#517).

class LevenbergMarquardt:
    """The least-squares optimizer used to fit model parameters."""

    def __init__(
        self,
        epsfcn: float = 1e-8,
        xtol: float = 1e-8,
        gtol: float = 1e-8,
        use_cost_functions_jacobian: bool = False,
    ) -> None: ...

class EndCriteria:
    """The optimizer stopping rule."""

    def __init__(
        self,
        max_iterations: int,
        max_stationary_state_iterations: int | None,
        root_epsilon: float,
        function_epsilon: float,
        gradient_norm_epsilon: float | None,
    ) -> None: ...
