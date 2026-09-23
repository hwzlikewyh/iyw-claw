use super::spawner::DelegationLink;

pub(crate) fn build(task: &str, link: &DelegationLink) -> String {
    format!(
        "You are executing delegated task {task_id} in an independent session. \
         The parent continues other work concurrently. Execute the task below now; \
         do not end your turn with only a role acknowledgment or a plan. \
         Read the relevant files and follow their repository instructions. \
         A brief progress update does not require a reply from the parent. \
         Return concrete findings or changes, validation evidence, and any blocker. \
         Review tasks do not require new files. Do not run checks forbidden by the repository. \
         If essential task details are missing, identify them explicitly; do not invent a task.\n\n\
         Task body:\n{task}",
        task_id = link.delegation_call_id,
    )
}
