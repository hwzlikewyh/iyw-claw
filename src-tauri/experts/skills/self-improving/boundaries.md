# Security Boundaries

Never store credentials, financial data, medical data, biometric data,
location routines, inferred sensitive traits, third-party personal data, or
access patterns.

Only explicit durable user corrections, preferences, and facts are eligible
for iyw-claw user-memory tools. Operational reflection files must not contain
user memory at all.

When the user asks to inspect, forget, delete, or export memory, use iyw-claw's
User Memory settings and supported host operations. Confirm destructive targets
precisely. Do not export a backup before a forget request unless the user asks,
and never manipulate the storage files with shell commands.
