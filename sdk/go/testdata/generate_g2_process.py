"""Capture unmodified QuantLib 1.43 regular G2 transitions and raw diffusion."""
import csv
import json
from pathlib import Path

import QuantLib as ql

assert ql.__version__ == "1.43"
root = Path(__file__).resolve().parents[3]
rows = []
with (root / "crates/libitofin/tests/data/g2_native.csv").open() as source:
    for row in csv.DictReader(source):
        r = {key: float(value) for key, value in row.items()}
        p = ql.G2Process(r["a"], r["sigma"], r["b"], r["eta"], r["rho"])
        x = [r["x"], r["y"]]
        matrix = lambda m: [[m[i][j] for j in range(2)] for i in range(2)]
        rows.append({
            "parameters": [r[k] for k in ["a", "sigma", "b", "eta", "rho"]],
            "t": r["t"], "dt": r["dt"], "state": x,
            "expectation": list(p.expectation(r["t"], x, r["dt"])),
            "covariance": matrix(p.covariance(r["t"], x, r["dt"])),
            "std_deviation": matrix(p.stdDeviation(r["t"], x, r["dt"])),
            "raw_diffusion": matrix(p.diffusion(r["t"], x)),
        })
result = {
    "oracle": "QuantLib 1.43 Python wheel, default empty term structure",
    "diffusion_qualification": "Raw unequal-volatility diffusion uses rho*sigma. Native libitofin deliberately uses rho*eta to preserve the second variance and cross covariance. Compare regular exact transitions separately; do not copy or normalize the raw diffusion defect.",
    "rows": rows,
}
Path(__file__).with_name("g2_process.json").write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
