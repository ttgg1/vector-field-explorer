// Task 3 of the preset-persistence plan: the app owns the preset data
// directory and passes it to the panel/export functions.

use vector_field_explorer::app::presets_data_dir;

#[test]
fn test_data_dir_honors_xdg_override() {
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: this is the only test in this file's process that touches the
    // variable, and no other thread may observe the mutation; `config_test` is
    // its own `[[test]]` target, so it runs in a per-file process.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", tmp.path()) };
    let dir = presets_data_dir();
    unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    assert_eq!(dir, tmp.path().join("vector-field-explorer"));
}
