use std::time::{Duration, Instant, SystemTime};

use sea_orm::DatabaseConnection;

use super::agent_retention_policy::{self, AgentLogTarget};
use super::agent_retention_scan::{self, LogGroup};
use crate::acp::agent_storage::{load_config, AgentStorageConfig, AgentStoragePaths};

pub(crate) const RETENTION_DAYS: u64 = 7;
const RETENTION_AGE: Duration = Duration::from_secs(RETENTION_DAYS * 24 * 60 * 60);
pub(crate) const TOTAL_BYTES_THRESHOLD: u64 = 256 * 1024 * 1024;
const CLEANUP_BUDGET: Duration = Duration::from_secs(2);
const CLEANUP_WALL_TIMEOUT: Duration = Duration::from_millis(2_250);

#[derive(Default)]
pub(crate) struct AgentLogCleanupReport {
    pub scanned_agents: usize,
    pub scanned_files: usize,
    pub total_bytes: u64,
    pub deleted_files: usize,
    pub deleted_bytes: u64,
    pub failed_files: usize,
    pub elapsed: Duration,
    pub decision: &'static str,
    pub first_error: Option<String>,
}

pub(crate) async fn cleanup_managed_agent_logs(conn: &DatabaseConnection) -> AgentLogCleanupReport {
    let started = Instant::now();
    let Some(paths) = AgentStoragePaths::active() else {
        return skipped(started, "storage_unavailable");
    };
    let config = match load_config(conn).await {
        Ok(Some(config)) => config,
        Ok(None) => AgentStorageConfig::confirmed(paths.root().clone()),
        Err(error) => return failed(started, "storage_unavailable", error.to_string()),
    };
    let targets = agent_retention_policy::targets(&paths, &config);
    let cleanup = tokio::task::spawn_blocking(move || cleanup_targets(targets));
    match tokio::time::timeout(CLEANUP_WALL_TIMEOUT, cleanup).await {
        Ok(Ok(mut report)) => {
            report.elapsed = started.elapsed();
            report
        }
        Ok(Err(error)) => failed(started, "cleanup_task_failed", error.to_string()),
        Err(_) => skipped(started, "cleanup_timed_out"),
    }
}

fn cleanup_targets(targets: Vec<AgentLogTarget>) -> AgentLogCleanupReport {
    let started = Instant::now();
    let deadline = started + CLEANUP_BUDGET;
    let scan = agent_retention_scan::collect_groups(targets, deadline);
    let mut report = report_from_scan(&scan);
    if scan.timed_out {
        report.decision = "cleanup_timed_out";
        return report;
    }
    if scan.groups.is_empty() {
        report.decision = decision(&report, false, false);
        return report;
    }
    let cutoff = SystemTime::now()
        .checked_sub(RETENTION_AGE)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let over_threshold = report.total_bytes > TOTAL_BYTES_THRESHOLD;
    let mut timed_out = false;
    for group in scan.groups {
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        if (over_threshold
            || (!group.reset_only && group.files.iter().any(|f| f.modified < cutoff)))
            && remove_group(group, cutoff, over_threshold, deadline, &mut report)
        {
            timed_out = true;
            break;
        }
    }
    report.decision = decision(&report, over_threshold, timed_out);
    report
}

fn remove_group(
    mut group: LogGroup,
    cutoff: SystemTime,
    over_threshold: bool,
    deadline: Instant,
    report: &mut AgentLogCleanupReport,
) -> bool {
    group.files.sort_by_key(|file| file.primary);
    let mut sidecar_failed = false;
    for file in group.files {
        if Instant::now() >= deadline {
            return true;
        }
        if !over_threshold && file.modified >= cutoff {
            continue;
        }
        if file.primary && sidecar_failed {
            continue;
        }
        match agent_retention_scan::remove_file(&group.allowed_dir, &file) {
            Ok(true) => {
                report.deleted_files += 1;
                report.deleted_bytes = report.deleted_bytes.saturating_add(file.size);
            }
            Ok(false) => {}
            Err(error) => {
                sidecar_failed |= !file.primary;
                record_failure(
                    report,
                    format!("{} {}: {error}", group.agent_type, group.group),
                );
            }
        }
    }
    false
}

fn decision(report: &AgentLogCleanupReport, over_threshold: bool, timed_out: bool) -> &'static str {
    if timed_out {
        "cleanup_timed_out"
    } else if report.failed_files > 0 {
        "cleanup_completed_with_errors"
    } else if report.deleted_files > 0 {
        "cleanup_completed"
    } else if report.scanned_files == 0 {
        "no_supported_logs"
    } else if !over_threshold {
        "below_threshold"
    } else {
        "cleanup_completed"
    }
}

fn record_failure(report: &mut AgentLogCleanupReport, error: impl Into<String>) {
    report.failed_files += 1;
    if report.first_error.is_none() {
        report.first_error = Some(error.into());
    }
}

fn report_from_scan(scan: &agent_retention_scan::AgentLogScanResult) -> AgentLogCleanupReport {
    AgentLogCleanupReport {
        scanned_agents: scan.scanned_agents,
        scanned_files: scan.scanned_files,
        total_bytes: scan.total_bytes,
        failed_files: scan.failed_files,
        first_error: scan.first_error.clone(),
        ..Default::default()
    }
}

fn skipped(started: Instant, decision: &'static str) -> AgentLogCleanupReport {
    AgentLogCleanupReport {
        elapsed: started.elapsed(),
        decision,
        ..Default::default()
    }
}

fn failed(started: Instant, decision: &'static str, error: String) -> AgentLogCleanupReport {
    AgentLogCleanupReport {
        elapsed: started.elapsed(),
        decision,
        failed_files: 1,
        first_error: Some(error),
        ..Default::default()
    }
}
