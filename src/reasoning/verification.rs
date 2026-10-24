//! Task-pinned optional host verifier invocation and its security-kernel audit.
use super::*;

impl Session {
    pub(crate) fn restore_verifier_provider(
        &mut self,
        provider: crate::verification::provider::Handle,
    ) {
        self.verifier_provider = Some(provider);
        self.verifier_provider_snapshot_written = true;
    }

    pub(crate) fn set_resource_route_scope(&mut self, scope: Option<String>) -> Result<()> {
        if scope.as_ref().is_some_and(|value| {
            value.len() != 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            bail!("Invalid resource route scope")
        }
        self.resource_route_scope = scope;
        Ok(())
    }

    pub(crate) fn policy_configuration_fingerprint(&self) -> Result<String> {
        let configuration = crate::escalation::empirical::configuration_fingerprint(&self.cfg)?;
        if let Some(route_scope) = &self.resource_route_scope {
            return Ok(crate::store::sha256(&serde_json::to_vec(&json!({
                "domain":"cognitive-fabric/effective-resource-route-configuration/v1",
                "configuration_sha256":configuration,
                "route_scope_sha256":route_scope,
                "verifier_provider":self.verifier_provider.as_ref().map(|provider|provider.descriptor()),
            }))?));
        }
        let Some(provider) = &self.verifier_provider else {
            return Ok(configuration);
        };
        Ok(crate::store::sha256(&serde_json::to_vec(&json!({
            "domain": "cognitive-fabric/effective-verifier-configuration/v1",
            "configuration_sha256": configuration,
            "verifier_provider": provider.descriptor(),
        }))?))
    }

    /// Persist the opt-in host verifier identity before any checker callback.
    /// The event is absent on the default compiled-only path.
    pub(crate) fn record_verifier_provider_snapshot(&mut self, query: &str) -> Result<()> {
        let Some(provider) = &self.verifier_provider else {
            return Ok(());
        };
        if !self.verification_enabled {
            return Ok(());
        }
        let Some(criterion) = crate::verification::Criterion::from_query(query)? else {
            return Ok(());
        };
        self.store.event(
            &self.task,
            "verification_provider_snapshot",
            "security-kernel",
            json!({
                "format_version": 1,
                "provider": provider.descriptor(),
                "criterion": criterion.name(),
                "objective_sha256": crate::store::sha256(query.as_bytes()),
                "compiled_checker_required": true,
                "permissions_granted": false,
                "implementation_attested": false
            }),
        )?;
        self.verifier_provider_snapshot_written = true;
        Ok(())
    }

    /// Charge one checker invocation, run the optional host claim under the
    /// original task deadline, and require the compiled criterion to pass too.
    pub(crate) async fn verify_candidate(
        &mut self,
        query: &str,
        criterion: &crate::verification::Criterion,
        answer: &str,
        cycle: Option<u32>,
        origin: Option<&str>,
        memory_id: Option<&str>,
    ) -> Result<(bool, Option<String>)> {
        self.authorize_tool(criterion.capability())?;
        let answer_hash = crate::store::sha256(answer.as_bytes());
        let objective_hash = crate::store::sha256(query.as_bytes());
        let compiled = criterion.check(answer);
        // A pinned typed verifier may provide a bounded diagnostic, but its
        // claim never replaces or weakens the compiled criterion check.
        if let Some((broker, pins)) = self.typed_modules.clone() {
            if let Some(request) =
                crate::reasoning::modules::candidate_verification_request(query, answer)?
            {
                let capability = request.capability();
                if let Some(candidates) = pins.get(&capability) {
                    let response = self
                        .dispatch_typed_module_pre_authorized(&broker, candidates, request)
                        .await;
                    let (selected_identity, claim, result_code) = match response {
                        Ok((pin, response)) => {
                            let identity = Some((
                                pin.descriptor().id.clone(),
                                pin.descriptor().version.clone(),
                            ));
                            let (claim, result) = match response {
                                crate::fabric::modules::Response::SortDiagnostic {
                                    claimed_valid,
                                }
                                | crate::fabric::modules::Response::OrderingDiagnostic {
                                    claimed_valid,
                                }
                                | crate::fabric::modules::Response::PathDiagnostic {
                                    claimed_valid,
                                } => (Some(claimed_valid), "diagnostic"),
                                _ => (None, "wrong_response_kind"),
                            };
                            (identity, claim, result)
                        }
                        Err(_) => (None, None, "unavailable"),
                    };
                    self.store.event(
                        &self.task,
                        "typed_verifier_result",
                        "fabric",
                        json!({
                            "format_version": 1,
                            "module_id": selected_identity.as_ref().map(|identity|&identity.0),
                            "module_version": selected_identity.as_ref().map(|identity|&identity.1),
                            "capability": capability.as_str(),
                            "objective_sha256": objective_hash,
                            "candidate_sha256": answer_hash,
                            "claim_valid": claim,
                            "result": result_code,
                            "compiled_checker_accepted": compiled.is_ok(),
                            "compiled_checker_authoritative": true,
                            "permissions_granted": false
                        }),
                    )?;
                }
            }
        }
        let provider = self.verifier_provider.clone();
        let mut provider_claim = None;
        let mut provider_reason_code = None;
        let verified = if let Some(provider) = provider {
            if !self.verifier_provider_snapshot_written {
                bail!("Verifier provider has no durable task identity snapshot")
            }
            // Authorization and its resource reservation are committed by
            // authorize_tool before this durable intent and before callback.
            self.store.event(
                &self.task,
                "verification_provider_intent",
                "security-kernel",
                json!({
                    "format_version": 1,
                    "provider": provider.descriptor(),
                    "criterion": criterion.name(),
                    "objective_sha256": objective_hash,
                    "answer_sha256": answer_hash,
                    "remaining_ms": self.deadline.saturating_duration_since(Instant::now()).as_millis().min(u64::MAX as u128) as u64,
                    "compiled_checker_required": true,
                    "permissions_granted": false
                }),
            )?;
            let started = Instant::now();
            let request = crate::verification::VerificationRequest {
                criterion: criterion.name().to_owned(),
                objective: query.to_owned(),
                objective_sha256: objective_hash.clone(),
                answer: answer.to_owned(),
                answer_sha256: answer_hash.clone(),
                deadline: self.deadline,
            };
            let claim_result = provider.verify(request).await;
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            let (claim_accepted, claim_reason) = match claim_result {
                Ok(claim) => (claim.accepted, claim.reason_code),
                // Provider error text may contain secrets or instructions. Keep
                // only a fixed rejection code and never send it to Jev-R.
                Err(_) => (
                    false,
                    Some(if Instant::now() >= self.deadline {
                        "provider_timeout".to_owned()
                    } else {
                        "provider_unavailable".to_owned()
                    }),
                ),
            };
            provider_claim = Some(claim_accepted);
            provider_reason_code = claim_reason.clone();
            let compiled_accepted = compiled.is_ok();
            let accepted = claim_accepted && compiled_accepted;
            let reason_code = if !compiled_accepted {
                "compiled_check_rejected"
            } else if !claim_accepted {
                claim_reason.as_deref().unwrap_or("provider_rejected")
            } else {
                "accepted"
            };
            self.store.event(
                &self.task,
                "verification_provider_result",
                "verifier-r",
                json!({
                    "format_version": 1,
                    "provider": provider.descriptor(),
                    "criterion": criterion.name(),
                    "objective_sha256": objective_hash,
                    "answer_sha256": answer_hash,
                    "provider_accepted": claim_accepted,
                    "compiled_accepted": compiled_accepted,
                    "verified": accepted,
                    "reason_code": reason_code,
                    "elapsed_ms": elapsed_ms,
                    "permissions_granted": false
                }),
            )?;
            accepted
        } else {
            compiled.is_ok()
        };
        let rejection = if verified {
            None
        } else if let Err(error) = &compiled {
            Some(error.to_string())
        } else {
            Some(format!(
                "Independent verifier provider rejected the candidate ({})",
                provider_reason_code
                    .as_deref()
                    .unwrap_or("provider_rejected")
            ))
        };
        let mut event = json!({
            "verified": verified,
            "criterion": criterion.name(),
            "expert": self.checker_binding,
            "objective_hash": objective_hash,
            "answer_hash": answer_hash,
            "reason": rejection,
        });
        if let Some(cycle) = cycle {
            event["cycle"] = json!(cycle);
        }
        if let Some(origin) = origin {
            event["origin"] = json!(origin);
        }
        if let Some(memory_id) = memory_id {
            event["memory_id"] = json!(memory_id);
        }
        if provider_claim.is_some() {
            event["provider"] = json!(self.verifier_provider.as_ref().unwrap().descriptor());
            event["provider_accepted"] = json!(provider_claim);
            event["compiled_accepted"] = json!(compiled.is_ok());
            event["provider_reason_code"] = json!(provider_reason_code);
        }
        self.store
            .event(&self.task, "verification", "verifier-r", event)?;
        if verified {
            self.verified = true;
            self.verified_criterion = Some(criterion.name());
            self.stop_reason = "verified".into();
        }
        Ok((verified, rejection))
    }
}
