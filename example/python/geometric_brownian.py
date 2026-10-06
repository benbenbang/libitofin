"""Inspect standalone arithmetic GBM coefficients and native Euler transitions."""

# itofin library
from itofin.processes import GeometricBrownianMotionProcess


def main() -> None:
    """Show signed deviation and a transition that crosses zero, not exact GBM."""
    process = GeometricBrownianMotionProcess(100.0, 0.05, 0.20)
    print(f"initial state: {process.x0():.10f}")
    print(f"mu: {process.mu():.10f}")
    print(f"volatility: {process.volatility():.10f}")
    print(f"arithmetic drift: {process.drift(0.0, 100.0):.10f}")
    print(f"arithmetic diffusion: {process.diffusion(0.0, 100.0):.10f}")
    print(f"Euler expectation: {process.expectation(0.0, 100.0, 0.25):.10f}")
    print(f"Euler variance: {process.variance(0.0, 100.0, 0.25):.10f}")
    print(f"signed Euler deviation: {process.std_deviation(0.0, -100.0, 0.25):.10f}")
    print(f"Euler transition: {process.evolve(0.0, 100.0, 0.25, -0.25):.10f}")
    print(f"Euler crossing zero: {process.evolve(0.0, 100.0, 1.0, -6.0):.10f}")


if __name__ == "__main__":
    main()
