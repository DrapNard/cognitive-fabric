//! Operator-configured role priority with a hard bound on queue bypasses.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorityScheduling {
    pub role_priorities: BTreeMap<String, u8>,
    pub max_bypasses: u16,
}
impl PriorityScheduling {
    pub(super) fn validate(&self) -> Result<()> {
        if self.role_priorities.is_empty()
            || self.role_priorities.len() > 7
            || !(1..=64).contains(&self.max_bypasses)
            || self
                .role_priorities
                .iter()
                .any(|(role, value)| crate::fabric::protocol::actor(role).is_err() || *value > 15)
        {
            bail!("Invalid resource priority scheduling policy")
        }
        Ok(())
    }
    pub(super) fn priority(&self, role: &str) -> u8 {
        self.role_priorities.get(role).copied().unwrap_or(0)
    }
}
impl Settings {
    pub(crate) fn validate_queue_audit(
        &self,
        role: &str,
        value: &Value,
        admitted: bool,
    ) -> Result<()> {
        match &self.scheduling {
            Some(policy) => {
                if value["queue_priority"].as_u64() != Some(u64::from(policy.priority(role)))
                    || (admitted
                        && value["queue_bypasses"]
                            .as_u64()
                            .is_none_or(|n| n > u64::from(policy.max_bypasses)))
                    || (!admitted && value.get("queue_bypasses").is_some())
                {
                    bail!("Original resource priority audit changed")
                }
            }
            None if value.get("queue_priority").is_some()
                || value.get("queue_bypasses").is_some() =>
            {
                bail!("Unexpected priority metadata for FIFO resource admission")
            }
            None => {}
        }
        Ok(())
    }
}
impl ResourcePool {
    pub(super) fn queue_index(&self, state: &State) -> Option<usize> {
        let policy = match &self.settings.scheduling {
            None => return (!state.queue.is_empty()).then_some(0),
            Some(policy) => policy,
        };
        // Once an older request reaches its bypass ceiling, later arrivals cannot
        // displace it again. Among protected requests the oldest always wins.
        if let Some(index) = state
            .queue
            .iter()
            .position(|id| state.entries[id].bypasses >= policy.max_bypasses)
        {
            return Some(index);
        }
        state
            .queue
            .iter()
            .enumerate()
            .max_by(|(ai, a), (bi, b)| {
                state.entries[*a]
                    .priority
                    .cmp(&state.entries[*b].priority)
                    .then_with(|| bi.cmp(ai))
            })
            .map(|(index, _)| index)
    }
}
