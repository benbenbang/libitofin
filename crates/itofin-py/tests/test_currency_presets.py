import csv
from pathlib import Path

import pytest

from itofin.indexes import Currency


with (Path(__file__).parent / "fixtures" / "currency_presets.tsv").open() as source:
    PRESETS = list(csv.DictReader(source, delimiter="\t"))


@pytest.mark.parametrize("row", PRESETS, ids=lambda row: row["code"])
def test_currency_presets_reachable(row):
    preset = getattr(Currency, row["code"].lower() + ("_" if row["code"] == "TRY" else ""))()
    assert preset.code() == row["code"]
    assert repr(preset) == f"Currency({row['code']})"
    assert getattr(Currency, row["code"].lower() + ("_" if row["code"] == "TRY" else ""))().code() == preset.code()


def test_currency_preset_scope_and_existing_four():
    assert len(PRESETS) == 20
    assert len({row["code"] for row in PRESETS}) == 20
    for code in ("EUR", "USD", "GBP", "JPY"):
        assert getattr(Currency, code.lower())().code() == code
    with pytest.raises(TypeError):
        getattr(Currency, "twd")(1)
