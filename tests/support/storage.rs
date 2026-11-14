//! In-memory eframe storage using the production RON codec.
use eframe::Storage as _;

#[derive(Default)]
pub struct MemoryStorage {
    pub values: std::collections::HashMap<String, String>,
}
impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.values.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.values.remove(key);
    }
    fn flush(&mut self) {}
}
pub fn ron_string(value: &impl serde::Serialize) -> String {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, "test", value);
    storage.get_string("test").unwrap()
}
pub fn from_ron<T: serde::de::DeserializeOwned>(raw: &str) -> T {
    let mut storage = MemoryStorage::default();
    storage.set_string("test", raw.to_owned());
    eframe::get_value(&storage, "test").unwrap()
}
