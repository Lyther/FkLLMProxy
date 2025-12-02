// Simple feature flag implementation based on environment variables
use std::collections::HashMap;
use std::env;
use std::sync::RwLock;
use tracing::info;

lazy_static::lazy_static! {
    static ref FLAGS: RwLock<HashMap<String, bool>> = RwLock::new(HashMap::new());
}

pub struct FeatureFlags;

impl FeatureFlags {
    /// Initialize flags from environment variables starting with FLAG_
    pub fn init() {
        let mut flags = FLAGS.write().expect("Feature flags lock poisoned - this indicates a serious bug");
        for (key, value) in env::vars() {
            if key.starts_with("FLAG_") {
                let flag_name = key
                    .strip_prefix("FLAG_")
                    .expect("FLAG_ prefix check failed - this should never happen")
                    .to_lowercase()
                    .replace('_', "-");
                let is_enabled = value.to_lowercase() == "true" || value == "1";
                flags.insert(flag_name.clone(), is_enabled);
                info!("Feature flag loaded: {} = {}", flag_name, is_enabled);
            }
        }
    }

    /// Check if a feature is enabled
    /// Returns false if flag doesn't exist
    pub fn is_enabled(flag: &str) -> bool {
        let flags = FLAGS.read().expect("Feature flags lock poisoned - this indicates a serious bug");
        *flags.get(flag).unwrap_or(&false)
    }

    /// Explicitly set a flag (useful for testing or dynamic updates)
    pub fn set(flag: &str, value: bool) {
        let mut flags = FLAGS.write().expect("Feature flags lock poisoned - this indicates a serious bug");
        flags.insert(flag.to_string(), value);
        info!("Feature flag updated: {} = {}", flag, value);
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
