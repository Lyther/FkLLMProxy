// Simple feature flag implementation based on environment variables
use std::collections::HashMap;
use std::env;
use std::sync::{OnceLock, RwLock};
use tracing::info;

static FLAGS: OnceLock<RwLock<HashMap<String, bool>>> = OnceLock::new();

pub struct FeatureFlags;

impl FeatureFlags {
    /// Initialize flags from environment variables starting with FLAG_
    pub fn init() {
        let flags_map = FLAGS.get_or_init(|| RwLock::new(HashMap::new()));
        let mut flags = flags_map
            .write()
            .expect("Failed to acquire write lock for flags init");

        let mut flag_count = 0;
        for (key, value) in env::vars() {
            if let Some(flag_name) = key.strip_prefix("FLAG_") {
                let flag_name = flag_name.to_lowercase().replace('_', "-");
                let is_enabled =
                    matches!(value.to_lowercase().as_str(), "true" | "1" | "yes" | "on");
                flags.insert(flag_name.clone(), is_enabled);
                flag_count += 1;
            }
        }
        if flag_count > 0 {
            info!("Loaded {} feature flag(s)", flag_count);
        }
    }

    /// Check if a feature is enabled
    /// Returns false if flag doesn't exist
    pub fn is_enabled(flag: &str) -> bool {
        let flags_map = FLAGS.get_or_init(|| RwLock::new(HashMap::new()));
        let flags = flags_map
            .read()
            .expect("Failed to acquire read lock for flags");
        flags.get(flag).copied().unwrap_or(false)
    }

    /// Explicitly set a flag (useful for testing or dynamic updates)
    pub fn set(flag: &str, value: bool) {
        let flags_map = FLAGS.get_or_init(|| RwLock::new(HashMap::new()));
        let mut flags = flags_map
            .write()
            .expect("Failed to acquire write lock for flags");
        flags.insert(flag.to_string(), value);
        info!("Feature flag updated: {} = {}", flag, value);
    }

    /// Reload flags from environment variables
    pub fn reload() {
        Self::init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_flags() {
        FeatureFlags::set("test-flag", true);
        assert!(FeatureFlags::is_enabled("test-flag"));

        FeatureFlags::set("test-flag", false);
        assert!(!FeatureFlags::is_enabled("test-flag"));

        assert!(!FeatureFlags::is_enabled("non-existent-flag"));
    }
}
