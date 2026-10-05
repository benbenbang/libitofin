"""Build the oracle only from archived pinned QuantLib sources."""

import hashlib
import io
import json
import subprocess
import tarfile
from pathlib import Path

COMMIT = "9863b578af0caa4cecabf697196533e84a8308b6"
SOURCES = [
    "ql/math/randomnumbers/seedgenerator.cpp",
    "ql/methods/montecarlo/brownianbridge.cpp",
    "ql/termstructures/volatility/equityfx/blackvoltimeextrapolation.cpp",
    "ql/errors.cpp",
    "ql/settings.cpp",
    "ql/event.cpp",
    "ql/instrument.cpp",
    "ql/exercise.cpp",
    "ql/stochasticprocess.cpp",
    "ql/interestrate.cpp",
    "ql/termstructure.cpp",
    "ql/patterns/observable.cpp",
    "ql/instruments/varianceswap.cpp",
    "ql/instruments/europeanoption.cpp",
    "ql/instruments/vanillaoption.cpp",
    "ql/instruments/oneassetoption.cpp",
    "ql/instruments/payoffs.cpp",
    "ql/pricingengines/vanilla/analyticeuropeanengine.cpp",
    "ql/pricingengines/blackcalculator.cpp",
    "ql/processes/blackscholesprocess.cpp",
    "ql/processes/eulerdiscretization.cpp",
    "ql/termstructures/yieldtermstructure.cpp",
    "ql/termstructures/yield/zeroyieldstructure.cpp",
    "ql/termstructures/yield/flatforward.cpp",
    "ql/termstructures/voltermstructure.cpp",
    "ql/termstructures/volatility/equityfx/blackvoltermstructure.cpp",
    "ql/termstructures/volatility/equityfx/blackvariancesurface.cpp",
    "ql/termstructures/volatility/equityfx/localvoltermstructure.cpp",
    "ql/termstructures/volatility/equityfx/localvolsurface.cpp",
    "ql/time/date.cpp",
    "ql/time/calendar.cpp",
    "ql/time/period.cpp",
    "ql/time/daycounters/actual365fixed.cpp",
    "ql/math/errorfunction.cpp",
    "ql/termstructures/volatility/smilesection.cpp",
    "ql/termstructures/volatility/equityfx/blackvariancecurve.cpp",
    "ql/utilities/dataformatters.cpp",
    "ql/pricingengines/blackformula.cpp",
    "ql/math/randomnumbers/mt19937uniformrng.cpp",
    "ql/math/distributions/normaldistribution.cpp",
    "ql/math/statistics/generalstatistics.cpp",
    "ql/timegrid.cpp",
    "ql/math/integrals/integral.cpp",
    "ql/math/integrals/segmentintegral.cpp",
]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(args):
    root = args.quantlib.resolve()
    commit = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if commit != COMMIT:
        raise ValueError(f"Expected QuantLib {COMMIT}, got {commit}")
    args.build_dir.mkdir(parents=True, exist_ok=True)
    source = args.build_dir / "source"
    source.mkdir(exist_ok=True)
    archive = subprocess.check_output(
        [
            "git",
            "-C",
            str(root),
            "archive",
            COMMIT,
            "ql",
            "test-suite/varianceswaps.cpp",
            "LICENSE.TXT",
        ]
    )
    with tarfile.open(fileobj=io.BytesIO(archive)) as contents:
        contents.extractall(source, filter="data")
    wrapper = Path(__file__).with_name("native.cpp").resolve()
    flags = ["-std=c++17", "-O2", "-ffunction-sections", "-fdata-sections"]
    includes = ["-I", str(source), "-I", str(args.boost_include)]
    objects, dependencies = [], set()
    for index, relative in enumerate([None, *SOURCES]):
        unit = wrapper if relative is None else source / relative
        obj = args.build_dir / f"unit-{index}.o"
        dep = args.build_dir / f"unit-{index}.d"
        subprocess.run(
            [
                args.cxx,
                *flags,
                *includes,
                "-MMD",
                "-MF",
                str(dep),
                "-c",
                str(unit),
                "-o",
                str(obj),
            ],
            check=True,
        )
        objects.append(str(obj))
        paths = dep.read_text().replace("\\\n", " ").split(":", 1)[1].split()
        for path in paths:
            candidate = Path(path)
            if candidate.is_relative_to(source):
                dependencies.add(candidate.relative_to(source).as_posix())
    binary = args.build_dir / "mc-variance-swap-native"
    subprocess.run(
        [args.cxx, "-Wl,-dead_strip", *objects, "-o", str(binary)], check=True
    )
    dependencies.add("test-suite/varianceswaps.cpp")
    compiler = subprocess.check_output([args.cxx, "--version"], text=True).splitlines()[
        0
    ]
    provenance = {
        "source_commit": COMMIT,
        "source_version": "1.43-dev",
        "source_sha256": {path: digest(source / path) for path in sorted(dependencies)},
        "compiled_sources": SOURCES,
        "compiler": compiler,
        "compiler_flags": flags,
        "linker_flags": ["-Wl,-dead_strip"],
        "generator_sha256": digest(Path(__file__).with_name("generate.py")),
        "build_helper_sha256": digest(Path(__file__)),
        "native_source_sha256": digest(wrapper),
    }
    (args.build_dir / "provenance.json").write_text(
        json.dumps(provenance, indent=2, sort_keys=True) + "\n"
    )
    return binary, provenance
