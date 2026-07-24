# Security Boundaries

Never store credentials, financial data, medical data, biometric data,
location routines, inferred sensitive traits, third-party personal data, or
access patterns.

Do not interrogate the user to complete a profile. Do not infer memory from
silence, tone, emotion, browsing, clicks, access patterns, or repeated Agent
guesses. Only information the user explicitly volunteers is eligible.

Keep one-time information in M0. Put other eligible reusable information in
M1 for review. Never auto-promote M1 because of repetition, confidence, or
convenience. M3 and M4 are host-owned syntheses and must never be written by
this skill.

Only explicit durable user corrections, preferences, goals, recurring
constraints, and facts are eligible for iyw-claw user-memory tools. Repository
details, temporary progress, and Agent private reasoning are ineligible.
Operational reflection files must not contain user memory at all.

When the user asks to inspect, forget, delete, or export memory, use iyw-claw's
User Memory settings and supported host operations. Confirm destructive targets
precisely. Do not export a backup before a forget request unless the user asks,
and never manipulate the storage files with shell commands.
