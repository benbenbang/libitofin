# Custom pillar oracle

`custom_pillars_oracle.cpp` links the repository's independent QuantLib 1.43-dev
checkout. It covers FRA, swap, OIS, zero-inflation and YoY-inflation custom pillars.
The fixture records exact dates and curve values; binding tests use absolute
`1e-12` numerical tolerance. Yield quotes also move from 3% to 3.1%.

```sh
c++ -std=c++17 -O2 -IQuantLib -I"$BOOST_INCLUDE" \
  sdk/go/testdata/custom_pillars_oracle.cpp -LQuantLib/build/ql -lQuantLib \
  -Wl,-rpath,"$PWD/QuantLib/build/ql" -o /tmp/custom-pillars-oracle
/tmp/custom-pillars-oracle | python3 -m json.tool --sort-keys --indent 2 \
  > sdk/go/testdata/custom_pillars.json
```

Custom pillars remain inside the helper's allowed date window. OIS `latestDate`
keeps its last relevant payment date, while FRA/swap `latestDate` follows the
pillar. Inflation `Linear` accepts a date inside the final fixing period; `Flat`
ignores the supplied valid custom date and keeps its single fixing date, as QL
does. Bindings require an explicit date for `CustomDate` and reject an unused
custom date with another convention. C configuration layouts are unchanged;
new `_with_pillar` entry points carry the date separately.
