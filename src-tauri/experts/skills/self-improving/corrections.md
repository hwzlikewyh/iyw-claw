# Correction Routing Notice

This bundled file is not a correction log. For an explicit reusable user
correction, call `propose_user_memory` with signal `correction`. If the user
explicitly asks to remember a durable preference or fact, call
`append_user_memory`.

Do not maintain a parallel corrections database under the self-improving state
directory.
