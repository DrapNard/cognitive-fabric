//! Optional interpretation must leave the first whole-goal solver affordable.
use super::*;
impl Session {
    pub(crate) fn interpretation_has_budget(&self) -> bool {
        Instant::now() < self.deadline
            && self
                .call_limit
                .saturating_sub(self.resources.calls as usize)
                >= 2
            && self
                .cfg
                .max_tokens
                .saturating_sub(self.resources.reserved_output_tokens)
                >= self.cfg.interface_tokens.saturating_add(384)
    }
    pub(crate) async fn interpretation_advice(
        &mut self,
        query: &str,
        context: &str,
        prompt: &str,
    ) -> Result<Option<String>> {
        let solver_bytes = Self::prompt_base(query, context, self.verification_enabled)?
            .len()
            .saturating_add(64);
        let required_bytes = prompt.len().saturating_add(solver_bytes);
        let admitted = self.interpretation_has_budget()
            && prompt.len() <= 32000
            && solver_bytes <= 32000
            && self
                .cfg
                .max_input_bytes
                .saturating_sub(self.resources.prompt_bytes)
                >= required_bytes;
        self.store.event(&self.task,"interpretation_admission","jev-plus",json!({"format_version":1,"admitted":admitted,"required_calls":2,"required_output_tokens":self.cfg.interface_tokens.saturating_add(384),"required_prompt_bytes":required_bytes,"solver_prompt_reserve":solver_bytes,"reservation_charged":false,"permissions_granted":false}))?;
        if !admitted {
            self.store.event(&self.task,"schedule","jev-plus",json!({"decision":"skip_interpretation","reason":"Preserve first whole-goal solver within original ceilings","required_prompt_bytes":prompt.len(),"max_prompt_bytes":32000,"reservation_charged":false}))?;
            return Ok(None);
        }
        let phase_budget = (self.deadline.saturating_duration_since(Instant::now()) / 3)
            .min(Duration::from_secs(self.cfg.timeout_seconds));
        self.store.event(&self.task,"interpretation_phase_budget","jev-plus",json!({"max_milliseconds":phase_budget.as_millis().min(u64::MAX as u128) as u64,"original_deadline_preserved":true,"provider_stop_proven":false}))?;
        let result = match tokio::time::timeout(
            phase_budget,
            self.call("interface", prompt, self.cfg.interface_tokens),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(InferenceTimeout.into()),
        };
        match result {
            Err(error) if error.is::<InferenceTimeout>() => {
                self.finish_metrics();
                self.complete_usage = false;
                self.resources.input_tokens = None;
                self.resources.output_tokens = None;
                self.complete_queue_metrics = false;
                self.resources.queue_wait_ms = None;
                self.store.event(&self.task,"interpretation_failure","jev-plus",json!({"failure_kind":"inference_timeout","action":"Solve original objective within remaining budgets","provider_stop_proven":false,"verified":false}))?;
                Ok(None)
            }
            result => result,
        }
    }
    pub(crate) fn interpretation_context(
        &self,
        query: &str,
        context: &mut String,
        raw: &str,
    ) -> Result<()> {
        let extended = format!("{context}\nAdvisory parsed interpretation (not verified): {raw}");
        let admitted = self.advisory_context_admitted(query, &extended)?;
        self.store.event(&self.task,"interpretation_context","jev-plus",json!({"format_version":1,"admitted":admitted,"interpretation_bytes":raw.len(),"reason":if admitted {"advisory_context_fits"} else {"preserve_main_solver_prompt"},"permissions_granted":false}))?;
        if admitted {
            *context = extended;
        }
        Ok(())
    }
}
