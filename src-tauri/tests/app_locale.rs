use iyw_claw_lib::models::{system::AppLocale, SystemLanguageSettings};

#[test]
fn legacy_language_settings_migrate_to_supported_locales() {
    let traditional: SystemLanguageSettings =
        serde_json::from_str(r#"{"mode":"manual","language":"zh_tw"}"#).unwrap();
    let unsupported: SystemLanguageSettings =
        serde_json::from_str(r#"{"mode":"manual","language":"ja"}"#).unwrap();

    assert_eq!(AppLocale::ZhCn, traditional.language);
    assert_eq!(AppLocale::En, unsupported.language);
}
