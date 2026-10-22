use libitofin::currency::Currency;

#[test]
fn selected_presets_match_all_six_source_fields() {
    let presets = [
        Currency::twd(),
        Currency::chf(),
        Currency::aud(),
        Currency::cad(),
        Currency::sek(),
        Currency::nok(),
        Currency::dkk(),
        Currency::nzd(),
        Currency::hkd(),
        Currency::sgd(),
        Currency::cny(),
        Currency::krw(),
        Currency::inr(),
        Currency::brl(),
        Currency::mxn(),
        Currency::zar(),
        Currency::pln(),
        Currency::czk(),
        Currency::huf(),
        Currency::try_(),
    ];
    let rows: Vec<_> = include_str!("currency_presets.tsv")
        .lines()
        .skip(1)
        .collect();
    assert_eq!(rows.len(), presets.len());
    for (preset, row) in presets.iter().zip(rows) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 10);
        assert_eq!(preset.name(), fields[1]);
        assert_eq!(preset.code(), fields[2]);
        assert_eq!(preset.numeric_code(), fields[3].parse::<i32>().unwrap());
        assert_eq!(preset.symbol(), fields[4]);
        assert_eq!(preset.fraction_symbol(), fields[5]);
        assert_eq!(
            preset.fractions_per_unit(),
            fields[6].parse::<i32>().unwrap()
        );
        assert_eq!(preset.to_string(), fields[2]);
        assert_eq!(*preset, Currency::new(fields[1], fields[2], 0, "", "", 1));
    }
}

#[test]
fn taiwan_preset_preserves_name_based_identity() {
    let twd = Currency::twd();
    assert_eq!(
        twd,
        Currency::new("Taiwan dollar", "TWD", 901, "NT$", "", 100)
    );
    assert_ne!(
        twd,
        Currency::new("New Taiwan Dollar", "TWD", 901, "NT$", "", 100)
    );
    assert_eq!(Currency::huf().fractions_per_unit(), 1);
    assert_eq!(Currency::jpy().fractions_per_unit(), 100);
}
