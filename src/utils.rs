//! Utility functions and a basic EventEmitter.

/// Converts a string with spaces into camelCase.
pub fn camelize(s: &str) -> String {
    let mut result = String::new();
    let mut uppercase_next = false;
    for c in s.chars() {
        if c.is_whitespace() {
            uppercase_next = true;
        } else if uppercase_next {
            result.push(c.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

/// Converts a camelCase string into one separated by the given separator.
pub fn uncamelize(s: &str, separator: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c.is_uppercase() {
            result.push_str(separator);
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    if result.starts_with(separator) {
        result[separator.len()..].to_string()
    } else {
        result
    }
}

/// Generates a random hexadecimal string of the specified length.
pub fn hex_string(len: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..len).map(|_| format!("{:x}", rng.gen_range(0..16))).collect()
}

/// A simple EventEmitter (for internal use and debugging).
pub struct EventEmitter {
    listeners: std::collections::HashMap<String, Vec<Box<dyn Fn(&serde_json::Value) + Send + Sync>>>,
}

impl EventEmitter {
    pub fn new() -> Self {
        Self {
            listeners: std::collections::HashMap::new(),
        }
    }

    /// Registers a listener for an event.
    pub fn on(&mut self, event: &str, callback: impl Fn(&serde_json::Value) + Send + Sync + 'static) {
        self.listeners
            .entry(event.to_string())
            .or_insert_with(Vec::new)
            .push(Box::new(callback));
    }

    /// Emits an event with associated data.
    pub fn emit(&self, event: &str, data: serde_json::Value) {
        if let Some(callbacks) = self.listeners.get(event) {
            for cb in callbacks {
                cb(&data);
            }
        }
    }
}
