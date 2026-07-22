//! Centralized logging configuration and deduplication manager for the simulation.
//!
//! Provides a unified logging framework for rate-limiting, deduplicating, and
//! toggling log entries across simulation phases.

use std::collections::HashMap;

/// Categories of simulation log messages.
///
/// Used to route log messages and control rate-limiting/deduplication settings
/// per simulation subsystem.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::logger::LogCategory;
///
/// let category = LogCategory::EmpireRelief;
/// assert!(category.config().dedup_enabled);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogCategory {
    /// Empire food relief alerts and treasury refunds.
    EmpireRelief,
    /// Lender of Last Resort (LLR) central bank emergency liquidity operations.
    LenderOfLastResort,
    /// Military and conflict resolution events.
    War,
    /// Alliance treaty changes and diplomatic alerts.
    Alliance,
    /// Random event generation and transitions.
    Event,
    /// High-level market and economic pulse summaries.
    EconomicPulse,
    /// General simulation notices.
    General,
}

/// Configuration settings for a log category.
///
/// Specifies whether deduplication is enabled and the tick interval threshold.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::logger::LogCategoryConfig;
///
/// let config = LogCategoryConfig {
///     dedup_enabled: true,
///     dedup_interval_ticks: 100,
/// };
/// assert!(config.dedup_enabled);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogCategoryConfig {
    /// Whether deduplication is enabled for this log category.
    pub dedup_enabled: bool,
    /// Minimum tick interval between duplicate log emissions.
    pub dedup_interval_ticks: u64,
}

impl LogCategory {
    /// Returns the central logging configuration for this category.
    ///
    /// Deduplication behavior (enabled status and tick window) can be easily toggled
    /// or modified per category in this central location.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::logger::LogCategory;
    ///
    /// let config = LogCategory::EmpireRelief.config();
    /// assert!(config.dedup_enabled);
    /// assert_eq!(config.dedup_interval_ticks, 100);
    /// ```
    pub fn config(&self) -> LogCategoryConfig {
        match self {
            LogCategory::EmpireRelief => LogCategoryConfig {
                dedup_enabled: true,
                dedup_interval_ticks: 100,
            },
            LogCategory::LenderOfLastResort => LogCategoryConfig {
                dedup_enabled: false,
                dedup_interval_ticks: 100,
            },
            LogCategory::War => LogCategoryConfig {
                dedup_enabled: false,
                dedup_interval_ticks: 0,
            },
            LogCategory::Alliance => LogCategoryConfig {
                dedup_enabled: false,
                dedup_interval_ticks: 0,
            },
            LogCategory::Event => LogCategoryConfig {
                dedup_enabled: false,
                dedup_interval_ticks: 0,
            },
            LogCategory::EconomicPulse => LogCategoryConfig {
                dedup_enabled: false,
                dedup_interval_ticks: 0,
            },
            LogCategory::General => LogCategoryConfig {
                dedup_enabled: false,
                dedup_interval_ticks: 0,
            },
        }
    }
}

/// Simulation log deduplicator tracking last emitted log timestamps.
///
/// Retains in-memory state tracking the last tick a message key was emitted for
/// each log category, allowing rate-limiting duplicate log messages.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::logger::{LogCategory, SimLogger};
///
/// let mut logger = SimLogger::new();
/// assert!(logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 1));
/// assert!(!logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 2));
/// assert!(logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 101));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimLogger {
    /// Maps `(category, key)` to the tick number it was last logged.
    last_logged: HashMap<(LogCategory, String), u64>,
}

impl SimLogger {
    /// Create a new, empty `SimLogger`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::logger::SimLogger;
    ///
    /// let logger = SimLogger::new();
    /// ```
    pub fn new() -> Self {
        Self {
            last_logged: HashMap::new(),
        }
    }

    /// Evaluates whether a log entry for `(category, key)` should be emitted at `current_tick`.
    ///
    /// If deduplication is enabled for `category` and the entry was logged less than
    /// `dedup_interval_ticks` ago, returns `false`. Otherwise, updates the last logged tick
    /// and returns `true`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::logger::{LogCategory, SimLogger};
    ///
    /// let mut logger = SimLogger::new();
    /// assert!(logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 1));
    /// assert!(!logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 2));
    /// assert!(logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 101));
    /// ```
    /// Evaluates whether a log entry for `(category, key)` should be emitted at `current_tick`
    /// given explicit configuration settings.
    ///
    /// Useful when testing or overriding configuration dynamically without depending
    /// on live category configuration defaults.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::logger::{LogCategory, LogCategoryConfig, SimLogger};
    ///
    /// let mut logger = SimLogger::new();
    /// let config = LogCategoryConfig {
    ///     dedup_enabled: true,
    ///     dedup_interval_ticks: 50,
    /// };
    /// assert!(logger.should_log_with_config(LogCategory::War, "combat_log", 1, config));
    /// assert!(!logger.should_log_with_config(LogCategory::War, "combat_log", 2, config));
    /// assert!(logger.should_log_with_config(LogCategory::War, "combat_log", 51, config));
    /// ```
    pub fn should_log_with_config(
        &mut self,
        category: LogCategory,
        key: &str,
        current_tick: u64,
        config: LogCategoryConfig,
    ) -> bool {
        if !config.dedup_enabled {
            return true;
        }

        let dedup_key = (category, key.to_string());
        if let Some(&last_tick) = self.last_logged.get(&dedup_key)
            && current_tick.saturating_sub(last_tick) < config.dedup_interval_ticks
        {
            return false;
        }

        self.last_logged.insert(dedup_key, current_tick);
        true
    }

    /// Evaluates whether a log entry for `(category, key)` should be emitted at `current_tick`.
    ///
    /// Delegates to [`SimLogger::should_log_with_config`] using the default [`LogCategory::config`]
    /// for `category`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::logger::{LogCategory, SimLogger};
    ///
    /// let mut logger = SimLogger::new();
    /// assert!(logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 1));
    /// assert!(!logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 2));
    /// assert!(logger.should_log(LogCategory::EmpireRelief, "emp_1_city_2", 101));
    /// ```
    pub fn should_log(&mut self, category: LogCategory, key: &str, current_tick: u64) -> bool {
        self.should_log_with_config(category, key, current_tick, category.config())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CATEGORIES: [LogCategory; 7] = [
        LogCategory::EmpireRelief,
        LogCategory::LenderOfLastResort,
        LogCategory::War,
        LogCategory::Alliance,
        LogCategory::Event,
        LogCategory::EconomicPulse,
        LogCategory::General,
    ];

    #[test]
    fn test_all_categories_with_enabled_mock_config() {
        let mock_enabled_config = LogCategoryConfig {
            dedup_enabled: true,
            dedup_interval_ticks: 100,
        };

        for cat in ALL_CATEGORIES {
            let mut logger = SimLogger::new();
            let key = "test_enabled_key";

            // Initial log at tick 1 -> allowed
            assert!(
                logger.should_log_with_config(cat, key, 1, mock_enabled_config),
                "Category {:?} should log on initial tick",
                cat
            );

            // Subsequent logs before 100 ticks -> suppressed
            assert!(
                !logger.should_log_with_config(cat, key, 2, mock_enabled_config),
                "Category {:?} should suppress duplicate at tick 2",
                cat
            );
            assert!(
                !logger.should_log_with_config(cat, key, 99, mock_enabled_config),
                "Category {:?} should suppress duplicate at tick 99",
                cat
            );

            // Log at tick 101 (100 ticks later) -> allowed again
            assert!(
                logger.should_log_with_config(cat, key, 101, mock_enabled_config),
                "Category {:?} should log again at tick 101",
                cat
            );
        }
    }

    #[test]
    fn test_all_categories_with_disabled_mock_config() {
        let mock_disabled_config = LogCategoryConfig {
            dedup_enabled: false,
            dedup_interval_ticks: 100,
        };

        for cat in ALL_CATEGORIES {
            let mut logger = SimLogger::new();
            let key = "test_disabled_key";

            // When deduplication is disabled, should log every tick regardless of interval
            assert!(
                logger.should_log_with_config(cat, key, 1, mock_disabled_config),
                "Category {:?} should log at tick 1",
                cat
            );
            assert!(
                logger.should_log_with_config(cat, key, 2, mock_disabled_config),
                "Category {:?} should log at tick 2 when dedup disabled",
                cat
            );
            assert!(
                logger.should_log_with_config(cat, key, 3, mock_disabled_config),
                "Category {:?} should log at tick 3 when dedup disabled",
                cat
            );
        }
    }

    #[test]
    fn test_live_category_configs_return_valid_structs() {
        for cat in ALL_CATEGORIES {
            let config = cat.config();
            // Verify live configuration produces a valid LogCategoryConfig
            let _ = config.dedup_enabled;
            let _ = config.dedup_interval_ticks;
        }
    }
}
