//! Token budgets: daily and monthly caps with warnings and hard stops.
//!
//! Limits live in settings (`budget.daily_tokens`, `budget.monthly_tokens`)
//! and are denominated in total tokens (input plus output). Zero or unset
//! means unlimited. Windows are rolling: the last 24 hours and the last 30
//! days. Every gated call checks before sending; usage is logged after the
//! call, so a burst of parallel calls can slightly overshoot — acceptable
//! for a single-user desktop app.

use el_core::{AppError, AppResult};
use rusqlite::Connection;

/// Settings key for the daily token cap.
pub const SETTING_DAILY_TOKENS: &str = "budget.daily_tokens";
/// Settings key for the monthly (rolling 30-day) token cap.
pub const SETTING_MONTHLY_TOKENS: &str = "budget.monthly_tokens";

const DAY_SECS: i64 = 24 * 3600;
const MONTH_SECS: i64 = 30 * DAY_SECS;

/// Fraction of a budget that triggers a warning instead of a block.
pub const WARNING_FRACTION: f64 = 0.8;

/// Outcome of a budget check.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetCheck {
    /// True when the call may proceed.
    pub allowed: bool,
    /// Set when usage crossed 80% of a configured cap.
    pub warning: Option<String>,
    /// Tokens used in the last 24 hours.
    pub day_used: i64,
    /// Tokens used in the last 30 days.
    pub month_used: i64,
}

/// Checks an estimated call cost against both budgets. Returns `Ok` with a
/// possible warning when the call may proceed, or `Err` when a cap blocks it.
pub fn check_budget(conn: &Connection, estimated_tokens: i64) -> AppResult<BudgetCheck> {
    if estimated_tokens < 0 {
        return Err(AppError::InvalidInput(
            "estimated tokens must not be negative".to_string(),
        ));
    }
    let now = chrono::Utc::now().timestamp();
    let (day_in, day_out) = el_db::repo::cache::sum_since(conn, now - DAY_SECS)?;
    let (month_in, month_out) = el_db::repo::cache::sum_since(conn, now - MONTH_SECS)?;
    let day_used = day_in + day_out;
    let month_used = month_in + month_out;

    let daily = read_limit(conn, SETTING_DAILY_TOKENS)?;
    let monthly = read_limit(conn, SETTING_MONTHLY_TOKENS)?;

    if let Some(limit) = daily {
        if day_used + estimated_tokens >= limit {
            return Err(AppError::InvalidInput(format!(
                "daily token budget exceeded ({day_used}/{limit} used); raise it in Settings or wait for the window to roll"
            )));
        }
    }
    if let Some(limit) = monthly {
        if month_used + estimated_tokens >= limit {
            return Err(AppError::InvalidInput(format!(
                "monthly token budget exceeded ({month_used}/{limit} used); raise it in Settings"
            )));
        }
    }

    let warning = warning_for("daily", daily, day_used)
        .or_else(|| warning_for("monthly", monthly, month_used));
    Ok(BudgetCheck {
        allowed: true,
        warning,
        day_used,
        month_used,
    })
}

fn warning_for(scope: &str, limit: Option<i64>, used: i64) -> Option<String> {
    let limit = limit?;
    if limit > 0 && used as f64 >= limit as f64 * WARNING_FRACTION {
        Some(format!(
            "{used}/{limit} {scope} tokens used (80% of budget)"
        ))
    } else {
        None
    }
}

fn read_limit(conn: &Connection, key: &str) -> AppResult<Option<i64>> {
    match el_db::repo::cache::get_setting(conn, key)? {
        None => Ok(None),
        Some(raw) => {
            let value: i64 = raw.trim().parse().map_err(|_| {
                AppError::Database(format!("invalid {key}: {raw:?} (expected an integer)"))
            })?;
            if value < 0 {
                return Err(AppError::Database(format!(
                    "invalid {key}: {value} (must not be negative)"
                )));
            }
            Ok(if value == 0 { None } else { Some(value) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDb {
        db: el_db::Database,
        path: std::path::PathBuf,
    }

    impl TestDb {
        fn setup() -> Self {
            let (db, path) = el_db::Database::open_ephemeral().expect("open test database");
            Self { db, path }
        }

        fn finish(self) {
            drop(self.db);
            std::fs::remove_file(self.path).ok();
        }
    }

    #[test]
    fn unlimited_by_default() {
        let test = TestDb::setup();
        let conn = test.db.connect().expect("connect");
        let check = check_budget(&conn, 10_000).expect("check");
        assert!(check.allowed);
        assert!(check.warning.is_none());
        drop(conn);
        test.finish();
    }

    #[test]
    fn hard_stop_at_daily_cap() {
        let test = TestDb::setup();
        let conn = test.db.connect().expect("connect");
        el_db::repo::cache::set_setting(&conn, SETTING_DAILY_TOKENS, "100").expect("set");
        el_db::repo::cache::log_usage(&conn, None, "openai", "gpt-x", 60, 30).expect("log");
        let blocked = check_budget(&conn, 20).expect_err("must block");
        assert!(blocked.to_string().contains("daily token budget exceeded"));
        // A smaller call still fits.
        assert!(check_budget(&conn, 5).expect("check").allowed);
        drop(conn);
        test.finish();
    }

    #[test]
    fn warning_at_eighty_percent() {
        let test = TestDb::setup();
        let conn = test.db.connect().expect("connect");
        el_db::repo::cache::set_setting(&conn, SETTING_DAILY_TOKENS, "100").expect("set");
        el_db::repo::cache::log_usage(&conn, None, "openai", "gpt-x", 50, 30).expect("log");
        let check = check_budget(&conn, 5).expect("check");
        assert!(check.allowed);
        assert!(check.warning.is_some());
        drop(conn);
        test.finish();
    }

    #[test]
    fn invalid_limit_rejected() {
        let test = TestDb::setup();
        let conn = test.db.connect().expect("connect");
        el_db::repo::cache::set_setting(&conn, SETTING_DAILY_TOKENS, "lots").expect("set");
        assert!(check_budget(&conn, 1).is_err());
        drop(conn);
        test.finish();
    }
}
