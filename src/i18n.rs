use std::collections::HashMap;
use once_cell::sync::Lazy;

pub static LOCALES: Lazy<HashMap<String, HashMap<String, String>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // load en.json
    let en_data = include_str!("../data/locales/en.json");
    let en_map: HashMap<String, String> = serde_json::from_str(en_data).expect("Failed to parse en.json");
    m.insert("en".to_string(), en_map);
    // load fr.json
    let fr_data = include_str!("../data/locales/fr.json");
    let fr_map: HashMap<String, String> = serde_json::from_str(fr_data).expect("Failed to parse fr.json");
    m.insert("fr".to_string(), fr_map);
    m
});

#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::LOCALES
            .get($crate::i18n::CURRENT_LOCALE)
            .and_then(|m| m.get($key))
            .cloned()
            .unwrap_or_else(|| format!("{{{}}}", $key))
    };
}

// Current locale, default to en
pub static CURRENT_LOCALE: &str = "en";