//! Analytics store lifecycle (DuckDB), compiled only with the `analytics`
//! feature. Opening never fails the app: if DuckDB cannot start, EndlessLi
//! runs without the analytical store and the Semantic Compression pipeline
//! stays disabled.

use std::path::Path;
use std::sync::Arc;

/// Opens (creating if needed) `analytics.duckdb` in `data_dir`. Returns
/// `None` and logs when the file cannot be opened.
pub fn open(data_dir: &Path) -> Option<Arc<el_analytics::Analytics>> {
    let path = data_dir.join(el_analytics::ANALYTICS_DB_FILENAME);
    match el_analytics::Analytics::open(&path) {
        Ok(analytics) => Some(Arc::new(analytics)),
        Err(err) => {
            tracing::warn!(error = %err, "analytics database unavailable; continuing without it");
            None
        }
    }
}
