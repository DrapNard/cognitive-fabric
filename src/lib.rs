extern crate alloc;

pub mod agent;
pub mod backend;
pub mod backups;
pub mod benchmarking;
mod capabilities;
pub mod config;
pub mod corpus;
pub mod diagnostics;
pub mod engine;
pub mod escalation;
pub mod evaluation;
pub mod events;
pub mod evolution;
mod execution_clock;
pub mod fabric;
pub mod gaps;
pub mod language;
pub mod math;
pub mod memory;
pub mod models;
pub mod module_tasks;
pub mod planning;
pub mod projects;
pub mod reasoning;
pub mod recovery;
pub mod registry;
pub mod residency;
pub mod resource_scheduler;
pub mod restore;
pub mod routing;
pub mod scheduling;
pub mod security;
pub mod security_evaluation;
pub mod service;
pub mod sql;
pub mod storage;
pub mod store;
pub mod strict_json;
mod structured;
pub mod telemetry;
pub mod verification;
pub use verification::{
    VerificationClaim, VerificationFuture, VerificationRequest, VerifierDescriptor,
    VerifierProvider,
};
pub mod wire;

pub mod gpu;

pub mod credentials;
mod rag_archive;
