# Contributing

Aetheria is a solo side project (see the README for the full "why"), so treat this more as "here's how to not waste your time" than a formal process.

## Issues

Bug reports and feature requests are welcome. For bugs, include:
- What you did, what you expected, what happened instead
- Server logs if it's a crash or a failed generation
- Browser console output if it's a frontend issue

## Pull requests

PRs are welcome for real bug fixes and small, self-contained improvements. Before starting on anything bigger, open an issue first, since the direction of this project is still mine to decide and I'd rather tell you that upfront than after you've written 500 lines.

A few things that'll get a PR merged faster:
- Keep it focused. One fix or one feature per PR, not a grab bag.
- Match the existing style. Look at neighboring code before writing new code.
- `cargo build --release -p server` and `trunk build --release --cargo-profile wasm-release` (in `crates/frontend`) should both succeed.
- No new dependencies unless there's a real reason existing ones can't do the job.

## What I'm not looking for

- Large refactors or architecture changes without prior discussion
- New LLM provider integrations unless they're genuinely popular and stable
- Anything that adds cloud calls or telemetry, keeping everything local is kind of the whole point of this project

If in doubt, open an issue and ask before you write code.
