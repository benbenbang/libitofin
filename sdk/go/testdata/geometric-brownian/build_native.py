"""Build the oracle only from archived pinned QuantLib sources."""

import hashlib
import io
import json
import subprocess
import tarfile
from pathlib import Path

COMMIT = "9863b578af0caa4cecabf697196533e84a8308b6"
SOURCES = [
    "ql/errors.cpp",
    "ql/processes/geometricbrownianprocess.cpp",
    "ql/processes/eulerdiscretization.cpp",
    "ql/stochasticprocess.cpp",
    "ql/patterns/observable.cpp",
    "ql/math/matrix.cpp",
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
    binary = args.build_dir / "geometric-brownian-native"
    subprocess.run(
        [args.cxx, "-Wl,-dead_strip", *objects, "-o", str(binary)], check=True
    )
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
