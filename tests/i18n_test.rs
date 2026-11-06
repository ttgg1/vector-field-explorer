#[cfg(test)]
mod tests {
    use vector_field_explorer::i18n;
    #[test]
    fn test_locale_loading() {
        // ensure en locale has expected keys
        let en = i18n::LOCALES.get("en").expect("en locale missing");
        assert_eq!(en.get("hello"), Some(&"Hello".to_string()));
        assert_eq!(en.get("goodbye"), Some(&"Goodbye".to_string()));

        let fr = i18n::LOCALES.get("fr").expect("fr locale missing");
        assert_eq!(fr.get("hello"), Some(&"Bonjour".to_string()));
    }
}
