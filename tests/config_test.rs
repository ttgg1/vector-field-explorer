// Task 3 of the preset-persistence plan: the app owns the preset data
// directory and passes it to the panel/export functions.

use vector_field_explorer::app::presets_data_dir;

#[test]
fn test_data_dir_honors_xdg_override() {
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: this is the only test in the suite process that touches the
    // variable, and no other test calls `presets_data_dir` (or otherwise reads
    // `XDG_CONFIG_HOME`), so no concurrent thread can observe the mutation.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", tmp.path()) };
    let dir = presets_data_dir();
    unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
    assert_eq!(dir, tmp.path().join("vector-field-explorer"));
}
