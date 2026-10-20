//! Fixed-label projections of hypothesis audit events, never answer quality or execution proof.
use super::*;
#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct HypothesisDiagnostics {
    accepted_portfolio_events: usize,
    legacy_advice_events: usize,
    invalid_portfolio_events: usize,
    selected_hypothesis_events: usize,
    exhausted_selection_events: usize,
    unclassified_events: usize,
}
impl HypothesisDiagnostics {
    pub(crate) fn add(&mut self, other: &Self) {
        self.accepted_portfolio_events += other.accepted_portfolio_events;
        self.legacy_advice_events += other.legacy_advice_events;
        self.invalid_portfolio_events += other.invalid_portfolio_events;
        self.selected_hypothesis_events += other.selected_hypothesis_events;
        self.exhausted_selection_events += other.exhausted_selection_events;
        self.unclassified_events += other.unclassified_events;
    }
}
pub(super) fn read(db: &rusqlite::Connection, task: &str) -> Result<HypothesisDiagnostics> {
    // Bounds apply before JSON projection. Only fixed labels leave SQLite, not
    // IDs, hashes, strategies, prompts, provider errors or future payload text.
    let mut statement = db.prepare(r#"
WITH bounded AS (
 SELECT kind,source,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END AS data
 FROM events WHERE task_id=?1 AND kind IN ('exploration_portfolio','hypothesis_selection')
 ORDER BY id LIMIT 129
)
SELECT CASE
 WHEN data IS NULL OR NOT json_valid(data) THEN NULL
 WHEN source IS NOT 'jev-r'
   OR json_type(data,'$.format_version') IS NOT 'integer'
   OR json_extract(data,'$.format_version')!=1
   OR json_type(data,'$.verified') IS NOT 'false'
   OR json_type(data,'$.permissions_granted') IS NOT 'false'
   OR json_type(data,'$.source_sha256') IS NOT 'text'
   OR length(CAST(json_extract(data,'$.source_sha256') AS BLOB))!=64
   OR json_extract(data,'$.source_sha256') GLOB '*[^0-9a-f]*'
 THEN 'unclassified'
 WHEN kind='exploration_portfolio' THEN CASE
   WHEN json_extract(data,'$.status')='legacy_advice' THEN 'legacy'
   WHEN json_extract(data,'$.status')='invalid_portfolio' THEN 'invalid'
   WHEN json_type(data,'$.status') IS NULL
     AND json_extract(data,'$.policy')='canonical-alternatives/v1'
     AND json_type(data,'$.utility_calibrated')='false'
     AND json_type(data,'$.alternative_ids')='array'
     AND json_array_length(data,'$.alternative_ids') BETWEEN 2 AND 3
     AND NOT EXISTS (SELECT 1 FROM json_each(data,'$.alternative_ids')
       WHERE type!='text' OR length(CAST(value AS BLOB)) NOT BETWEEN 1 AND 16
       OR value GLOB '*[^A-Za-z0-9_]*')
     AND (SELECT count(DISTINCT value) FROM json_each(data,'$.alternative_ids'))=json_array_length(data,'$.alternative_ids')
   THEN 'accepted' ELSE 'unclassified' END
 WHEN kind='hypothesis_selection'
   AND json_extract(data,'$.policy')='canonical-alternatives/v1'
   AND json_type(data,'$.utility_calibrated')='false'
   AND json_type(data,'$.cycle')='integer'
   AND json_extract(data,'$.cycle') BETWEEN 1 AND 16
 THEN CASE
   WHEN json_type(data,'$.portfolio_exhausted')='true'
     AND json_type(data,'$.selected_id')='null'
     AND json_type(data,'$.strategy_sha256')='null' THEN 'exhausted'
   WHEN json_type(data,'$.portfolio_exhausted')='false'
     AND json_type(data,'$.selected_id')='text'
     AND length(CAST(json_extract(data,'$.selected_id') AS BLOB)) BETWEEN 1 AND 16
     AND json_extract(data,'$.selected_id') NOT GLOB '*[^A-Za-z0-9_]*'
     AND json_type(data,'$.strategy_sha256')='text'
     AND length(CAST(json_extract(data,'$.strategy_sha256') AS BLOB))=64
     AND json_extract(data,'$.strategy_sha256') NOT GLOB '*[^0-9a-f]*'
   THEN 'selected' ELSE 'unclassified' END
 ELSE 'unclassified' END FROM bounded
"#)?;
    let mut rows = statement.query([task])?;
    let mut result = HypothesisDiagnostics::default();
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 128 {
            bail!("Hypothesis diagnostic audit exceeds event limit")
        }
        let label: String = row.get(0)?;
        match label.as_str() {
            "accepted" => result.accepted_portfolio_events += 1,
            "legacy" => result.legacy_advice_events += 1,
            "invalid" => result.invalid_portfolio_events += 1,
            "selected" => result.selected_hypothesis_events += 1,
            "exhausted" => result.exhausted_selection_events += 1,
            "unclassified" => result.unclassified_events += 1,
            _ => bail!("Invalid hypothesis diagnostic label"),
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "hypothesis_diagnostics_tests.rs"]
mod tests;
