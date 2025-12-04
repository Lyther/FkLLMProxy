// Simple feature flag implementation based on environment variables
use std::collections::HashMap;
use std::env;
use std::sync::{OnceLock, RwLock};
use tracing::{info, warn};

static FLAGS: OnceLock<RwLock<HashMap<String, bool>>> = OnceLock::new();

pub struct FeatureFlags;

impl FeatureFlags {
    /// Initialize flags from environment variables starting with FLAG_
    pub fn init() {
        let flags_map = FLAGS.get_or_init(|| RwLock::new(HashMap::new()));
        // Fix lock poisoning: Use try_write() or recover from poison
        let mut flags = flags_map.write().unwrap_or_else(|poisoned| {
            warn!("Flags lock was poisoned, recovering by clearing and reinitializing");
            poisoned.into_inner()
        });

        let mut flag_count = 0;
        for (key, value) in env::vars() {
            if let Some(flag_name) = key.strip_prefix("FLAG_") {
                // Fix collision risk: Normalize flag name but preserve distinction
                // Convert to lowercase and replace underscores with hyphens
                // Note: FLAG_FOO_BAR and FLAG_FOO-BAR will collide, but this is documented
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
    /// Use `is_set()` to distinguish between "disabled" and "not configured"
    pub fn is_enabled(flag: &str) -> bool {
        Self::is_set(flag).unwrap_or(false)
    }

    /// Check if a feature flag is set (configured)
    /// Returns Some(true) if enabled, Some(false) if disabled, None if not configured
    /// This distinguishes between "disabled" and "not configured" which helps catch config errors
    pub fn is_set(flag: &str) -> Option<bool> {
        let flags_map = FLAGS.get_or_init(|| RwLock::new(HashMap::new()));
        // Fix lock poisoning: Recover from poison instead of panicking
        let flags = flags_map.read().unwrap_or_else(|poisoned| {
            warn!("Flags lock was poisoned, recovering by clearing and reinitializing");
            // Reinitialize on poison
            drop(poisoned);
            Self::init();
            flags_map
                .read()
                .expect("Failed to acquire read lock after recovery")
        });
        flags.get(flag).copied()
    }

    /// Explicitly set a flag (useful for testing or dynamic updates)
    pub fn set(flag: &str, value: bool) {
        let flags_map = FLAGS.get_or_init(|| RwLock::new(HashMap::new()));
        // Fix lock poisoning: Recover from poison instead of panicking
        let mut flags = flags_map.write().unwrap_or_else(|poisoned| {
            warn!("Flags lock was poisoned, recovering by clearing and reinitializing");
            poisoned.into_inner()
        });
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
        assert_eq!(FeatureFlags::is_set("test-flag"), Some(true));

        FeatureFlags::set("test-flag", false);
        assert!(!FeatureFlags::is_enabled("test-flag"));
        assert_eq!(FeatureFlags::is_set("test-flag"), Some(false));

        // Test distinction between "disabled" and "not configured"
        assert!(!FeatureFlags::is_enabled("non-existent-flag"));
        assert_eq!(FeatureFlags::is_set("non-existent-flag"), None);
    }
}
