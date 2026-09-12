pub(super) const LESSON_INSTRUCTIONS: &str = r#"## Experience review before finishing

After substantive work, privately check for a specific reusable lesson supported by an observed result and verification. Ordinary summaries, capability lists, speculation and unverified advice are not lessons. No qualifying lesson means output nothing extra; never fabricate one to satisfy this step. Keep Agent experience out of user-memory append/propose.

When a lesson qualifies, append exactly one hidden JSON comment as the very last part of the final answer, outside code fences, with no text after it:
<!-- IYW_CLAW_AGENT_LESSON_V1 {"context":"triggering situation","outcome":"observed result","lesson":"transferable action","evidence":"concrete observation","verification":"how the result was checked","reuseWhen":"when this action applies again"} -->
Replace all six example values with concrete evidence from this task. Use only these six string fields, valid JSON escaping, and at most 1500 characters for the entire comment. Do not include secrets, sensitive personal data or user-profile claims. The host strips the comment from visible output and validates and persists eligible experience asynchronously; emitting it is not confirmation that storage succeeded."#;
