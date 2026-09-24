// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Validation of OS-attested sandbox job process lifetimes.

use std::collections::HashSet;

use learning_mode_core::{AnalyzeError, ProcessLifetime};

/// Maximum number of process generations accepted for one guarded capture.
pub const MAX_JOB_PROCESS_LIFETIMES: usize = 4096;

/// One process generation attested by a job notification and retained handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobProcessMembership {
    /// Process identifier reported by the job object.
    pub pid: u32,
    /// Exact creation time read from a retained process handle.
    pub creation_filetime: u64,
    /// Exact exit time read from the same retained process handle.
    pub exit_filetime: u64,
    /// Ordered position of `JOB_OBJECT_MSG_NEW_PROCESS`.
    pub start_sequence: usize,
    /// FILETIME when the guardian received the new-process message.
    pub start_observed_filetime: u64,
    /// Ordered position of the exit notification, when the port delivered one.
    pub end_sequence: Option<usize>,
    /// FILETIME when the guardian received the exit-process or active-zero message.
    pub end_observed_filetime: u64,
}

/// Bounded job membership evidence captured by the elevated guardian.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobMembershipSnapshot {
    /// Exact root generation attested from the duplicated process handle.
    pub root_process: ProcessLifetime,
    /// FILETIME immediately before the job was associated with the completion port.
    pub attached_filetime: u64,
    /// FILETIME when `JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO` was received.
    pub completed_filetime: u64,
    /// Kernel accounting count for all process generations ever assigned to the job.
    pub total_processes: u32,
    /// Number of ordered PID-bearing completion-port notifications retained.
    pub notification_count: usize,
    /// Completed descendant generations attested by retained process handles.
    pub processes: Vec<JobProcessMembership>,
}

/// Validates job evidence and returns exact handle-attested process lifetimes.
///
/// Every descendant handle is opened from a job new-process notification and
/// verified against the duplicated sandbox job. Retaining the handle pins that
/// exact process generation even after exit, so PID reuse does not require ETL
/// lifecycle inference.
pub(crate) fn attested_process_lifetimes(
    membership: &JobMembershipSnapshot,
) -> Result<Vec<ProcessLifetime>, AnalyzeError> {
    validate_membership(membership)?;

    let mut lifetimes = Vec::with_capacity(membership.processes.len() + 1);
    lifetimes.push(membership.root_process);
    lifetimes.extend(membership.processes.iter().map(|process| ProcessLifetime {
        pid: process.pid,
        start_filetime: process.creation_filetime,
        end_filetime: process.exit_filetime,
    }));
    lifetimes.sort_unstable_by_key(|lifetime| lifetime.start_filetime);
    Ok(lifetimes)
}

fn validate_membership(membership: &JobMembershipSnapshot) -> Result<(), AnalyzeError> {
    if membership.root_process.pid == 0
        || membership.root_process.start_filetime == 0
        || membership.root_process.end_filetime < membership.root_process.start_filetime
    {
        return Err(AnalyzeError::Decode(
            "guarded sandbox root process generation is invalid".to_string(),
        ));
    }
    let retained_processes = membership.processes.len() + 1;
    if retained_processes > MAX_JOB_PROCESS_LIFETIMES {
        return Err(AnalyzeError::Decode(format!(
            "guarded sandbox job exceeded the {MAX_JOB_PROCESS_LIFETIMES}-process limit"
        )));
    }
    if u32::try_from(retained_processes).ok() != Some(membership.total_processes) {
        return Err(AnalyzeError::Decode(format!(
            "guarded sandbox job accounting reported {} process generation(s), but {} unique \
             generation(s) were retained",
            membership.total_processes, retained_processes
        )));
    }
    if membership.completed_filetime < membership.attached_filetime {
        return Err(AnalyzeError::Decode(
            "guarded sandbox job completion predates job attachment".to_string(),
        ));
    }
    if membership.root_process.end_filetime > membership.completed_filetime {
        return Err(AnalyzeError::Decode(
            "guarded sandbox root exit follows job completion".to_string(),
        ));
    }

    let mut generations = HashSet::with_capacity(retained_processes);
    generations.insert((
        membership.root_process.pid,
        membership.root_process.start_filetime,
    ));
    let mut sequences = Vec::with_capacity(membership.notification_count);
    for process in &membership.processes {
        if process.pid == 0
            || process.creation_filetime < membership.attached_filetime
            || process.exit_filetime < process.creation_filetime
            || process.creation_filetime > process.start_observed_filetime
            || process.exit_filetime > process.end_observed_filetime
            || process.exit_filetime > membership.completed_filetime
        {
            return Err(AnalyzeError::Decode(format!(
                "job-attested PID {} has an invalid handle-attested lifetime",
                process.pid
            )));
        }
        if !generations.insert((process.pid, process.creation_filetime)) {
            return Err(AnalyzeError::Decode(format!(
                "job-attested PID {} repeats an already retained process generation",
                process.pid
            )));
        }
        sequences.push(process.start_sequence);
        if let Some(end_sequence) = process.end_sequence {
            if end_sequence <= process.start_sequence {
                return Err(AnalyzeError::Decode(format!(
                    "job-attested PID {} has an exit notification before its start notification",
                    process.pid
                )));
            }
            sequences.push(end_sequence);
        }
    }
    sequences.sort_unstable();
    if sequences.len() != membership.notification_count {
        return Err(AnalyzeError::Decode(
            "guarded sandbox job notification count is inconsistent".to_string(),
        ));
    }
    if sequences
        .iter()
        .copied()
        .enumerate()
        .any(|(expected, actual)| expected != actual)
    {
        return Err(AnalyzeError::Decode(
            "guarded sandbox job membership notification order is incomplete".to_string(),
        ));
    }
    Ok(())
}
