//! Opt-in, same-boot recent-failure admission for optional advice. Never a permission or utility proof.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub min_observations: usize,
    pub sample_size: usize,
    pub window_seconds: u64,
    pub trip_failure_basis_points: u32,
}
impl Settings {
    pub(crate) fn validate(&self) -> Result<()> {
        if !(1..=16).contains(&self.min_observations)
            || !(self.min_observations..=16).contains(&self.sample_size)
            || !(1..=3600).contains(&self.window_seconds)
            || !(1..=10000).contains(&self.trip_failure_basis_points)
        {
            bail!("Invalid optional advice guard settings")
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Planning,
    Exploration,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Outcome {
    Completed,
    TimedOut,
    Failed,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    format_version: u32,
    scope_sha256: String,
    phase: Phase,
    boot_id: String,
    ended_ns: u64,
    budget_ms: u64,
    elapsed_ms: u64,
    outcome: Outcome,
    permissions_granted: bool,
    provider_stop_proven: bool,
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Observation {
    fn validate(&self) -> Result<()> {
        if self.format_version != 1
            || !hash(&self.scope_sha256)
            || uuid::Uuid::parse_str(&self.boot_id)?.to_string() != self.boot_id
            || !(1..=300000).contains(&self.budget_ms)
            || self.elapsed_ms > 3_600_000
            || self.permissions_granted
            || self.provider_stop_proven
        {
            bail!("Invalid optional advice observation")
        }
        Ok(())
    }
}
pub(super) struct Ticket {
    phase: Phase,
    scope: Option<String>,
    budget: Duration,
    started: Instant,
    calls_before: u32,
}
impl Ticket {
    pub(super) fn remaining(&self) -> Duration {
        self.budget.saturating_sub(self.started.elapsed())
    }
}
#[derive(Default, Serialize)]
struct History {
    observations: usize,
    failures: usize,
    total_elapsed_ms: u64,
}
fn tripped(settings: &Settings, counts: &History) -> bool {
    counts.observations >= settings.min_observations
        && counts.failures * 10000
            >= counts.observations * settings.trip_failure_basis_points as usize
}

fn history(
    db: &rusqlite::Connection,
    settings: &Settings,
    scope: &str,
    phase: Phase,
    budget_ms: u64,
    boot: &str,
    now: u64,
) -> Result<History> {
    // Inspect exactly a bounded tail of ALL events before filtering scope/kind.
    // Do not scan an unbounded global history seeking enough matching samples.
    let mut stmt=db.prepare("WITH tail AS (SELECT id,kind,source,payload FROM events ORDER BY id DESC LIMIT 512) SELECT CASE WHEN source='jev-plus' THEN 'jev-plus' END,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM tail WHERE kind='advice_phase_observation' ORDER BY id DESC")?;
    let mut rows = stmt.query([])?;
    let mut result = History::default();
    while let Some(row) = rows.next()? {
        let source: String = row.get(0)?;
        let raw: String = row.get(1)?;
        let observation: Observation =
            serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 4096)?)?;
        if source != "jev-plus" {
            bail!("Invalid optional advice observation source")
        }
        observation.validate()?;
        if observation.boot_id != boot {
            continue;
        }
        if observation.ended_ns > now {
            continue;
        }
        if observation.scope_sha256 != scope
            || observation.phase != phase
            || now - observation.ended_ns > settings.window_seconds * 1_000_000_000
            || observation.budget_ms.div_ceil(1000) < budget_ms.div_ceil(1000)
        {
            continue;
        }
        result.observations += 1;
        result.failures += usize::from(
            !matches!(observation.outcome, Outcome::Completed)
                || observation.elapsed_ms > budget_ms,
        );
        result.total_elapsed_ms += observation.elapsed_ms;
        if result.observations == settings.sample_size {
            break;
        }
    }
    Ok(result)
}
impl Session {
    pub(super) fn advice_ticket(
        &self,
        phase: Phase,
        mode: &str,
        base: &str,
    ) -> Result<Option<Ticket>> {
        let budget = (self.deadline.saturating_duration_since(Instant::now()) / 3)
            .min(Duration::from_secs(self.cfg.timeout_seconds));
        let started = Instant::now();
        let Some(settings) = &self.cfg.advice_guard else {
            return Ok(Some(Ticket {
                phase,
                scope: None,
                budget,
                started,
                calls_before: self.resources.calls,
            }));
        };
        let mut modules = self.backend.modules()?;
        if modules.len() > 64 {
            bail!("Optional advice module scope exceeds bounds")
        }
        for module in &mut modules {
            module.validate()?;
            module.roles.sort();
        }
        modules.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.version.cmp(&b.version)));
        let module_bytes = serde_json::to_vec(&modules)?;
        if module_bytes.len() > 16384 {
            bail!("Optional advice module scope exceeds byte bound")
        }
        let scope = crate::store::sha256(&serde_json::to_vec(&json!({
            "format_version":1,"configuration":crate::escalation::empirical::configuration_fingerprint(&self.cfg)?,
            "prompts":crate::backend::runtime_role_prompts_sha256()?,"runtime":crate::escalation::empirical::runtime_sources_fingerprint(),
            "modules":crate::store::sha256(&module_bytes),"mode":mode,"execution_profile":self.advice_profile,
            "criterion_binding":self.escalation_criterion,"prompt_kib_bucket":base.len().div_ceil(1024)
        }))?);
        let budget_ms = budget.as_millis().min(u64::MAX as u128) as u64;
        let (counts, allow, reason) =
            if let Some((boot, now)) = crate::execution_clock::observation() {
                let counts = history(
                    &*self.store.lock()?,
                    settings,
                    &scope,
                    phase,
                    budget_ms,
                    &boot,
                    now,
                )?;
                let trip = tripped(settings, &counts);
                (
                    counts,
                    !trip,
                    if trip {
                        "recent_failures"
                    } else {
                        "recent_history_allows"
                    },
                )
            } else {
                (History::default(), false, "clock_unavailable")
            };
        self.store.event(&self.task,"advice_guard_decision","jev-plus",json!({"format_version":1,"policy":"recent-advice-failures/v1","phase":phase,"scope_sha256":scope,"allow":allow,"reason":reason,"history":counts,"budget_ms":budget_ms,"permissions_granted":false,"utility_calibrated":false,"provider_stop_proven":false}))?;
        Ok(allow.then_some(Ticket {
            phase,
            scope: Some(scope),
            budget,
            started,
            calls_before: self.resources.calls,
        }))
    }
    pub(super) fn observe_advice(&self, ticket: &Ticket, outcome: Outcome) -> Result<()> {
        if self.resources.calls == ticket.calls_before {
            return Ok(());
        }
        let Some(scope) = &ticket.scope else {
            return Ok(());
        };
        let (boot_id, ended_ns) = crate::execution_clock::observation()
            .ok_or_else(|| anyhow::anyhow!("Optional advice clock unavailable after dispatch"))?;
        let observation = Observation {
            format_version: 1,
            scope_sha256: scope.clone(),
            phase: ticket.phase,
            boot_id,
            ended_ns,
            budget_ms: (ticket.budget.as_millis() as u64).max(1),
            elapsed_ms: ticket.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            outcome,
            permissions_granted: false,
            provider_stop_proven: false,
        };
        observation.validate()?;
        self.store.event(
            &self.task,
            "advice_phase_observation",
            "jev-plus",
            serde_json::to_value(observation)?,
        )?;
        Ok(())
    }
}
#[cfg(test)]
#[path = "advice_guard_tests.rs"]
mod tests;
