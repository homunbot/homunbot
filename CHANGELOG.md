# Changelog

All notable changes to Homun are documented here, in the format the marketing site's changelog
parses. This is the single source of truth: the released version's section is written into the
GitHub Release body (from which the app shows the in-app "What's new" on update, and the website
`/changelog` renders it via the GitHub Releases API).

Section headers are `## Highlights` / `## Improvements` / `## Fixes` (H2), and each bullet is a
single line so the site captures its full text; version delimiters are `## [x.y.z] — date`.

## [Unreleased]

## [0.1.1100] — 2026-09-07

A focused release that makes the assistant ask for approval before multi-step work instead of acting unilaterally.

## Highlights
- **The assistant now proposes a plan for approval before complex work.** For non-trivial multi-step requests that create or mutate things, Homun defers the work tools and asks the model to propose a plan the user can accept or edit, instead of choosing a language or folder and proceeding on its own.

## Improvements
- **Read-only and single-step requests still pass straight through.** Browsing, searching, reading and simple one-shot actions skip the approval step, so everyday use stays fast.

## Fixes
- **The plan-approval flow is now actually reachable.** The plan-proposal card (Accept/Edit) was wired end-to-end but never activated; the gate now triggers it instead of bootstrapping a generic plan and proceeding.

## [0.1.1099] — 2026-09-01

A production-readiness hardening release for packaged smoke reliability, real-profile audit clarity and browser automation supply-chain checks.

## Highlights
- **Packaged smoke now covers the full realistic scenario matrix.** The rebuilt package has passed chat, browser, automation, MCP, skill/tool selection, memory/privacy, code-routing, payment-approval and Italian web-discovery scenarios.
- **Real-profile integrity audit now separates hard failures from delivered historical plan debt.** Completed turns with delivered assistant output but stale open plan UI are visible as warnings instead of blocking the audit as unresolved runtime failures.
- **Browser automation dependencies are audited as part of the release gate.** The pre-release gate now checks the packaged sidecar dependency surface, not only the Electron app dependencies.

## Improvements
- **Scenario selection now fails closed.** Requesting an extended smoke scenario outside the selected profile exits non-zero instead of producing a false green run.
- **Package smoke avoids channel sidecar ports.** The package smoke gateway now uses `127.0.0.1:18768`, away from the WhatsApp and Telegram bridge ports.
- **Audit timeline output remains bounded.** Noisy browser or long chat histories keep terminal evidence visible without flooding the CLI report.

## Fixes
- **Delivered numbered reports close their final plan step.** The runtime now recognizes sourced numbered reports as delivered evidence, not only markdown tables.
- **Deleted chat threads purge their scoped runtime rows.** Smoke and real-profile cleanup no longer leave orphaned chat turn state behind.
- **The packaged browser automation sidecar no longer ships vulnerable `nanoid`.** The sidecar locks `nanoid` to `3.3.18` and both source and staged package audits report zero high-severity vulnerabilities.

## [0.1.1098] — 2026-09-01

A production hardening release for real-profile integrity, browser automation failures and model-selection clarity.

## Highlights
- **Real profile integrity can now be audited and repaired.** Homun can detect stale streaming turns, orphaned approvals and browser tasks that exhausted their action budget, then close them canonically with backups.
- **Model selection is easier to trust.** The composer treats `Unavailable` as a placeholder, keeps `Auto` visible for automatic routing and records the effective model choice as routing metadata.
- **Browser automation failures are more explicit.** Browser tasks that make no semantic progress now end as failures with preserved evidence instead of looking like successful assistant text.

## Improvements
- **Production smoke coverage now includes longer agentic scenarios.** The local release gate covers browser no-progress, long-process checkpointing, subagent probes, automation dry-runs and runtime integrity checks.
- **Packaged builds use the right gateway and token contract.** Desktop packaging and development startup now verify the bundled gateway path and keep the gateway token outside the renderer bundle.
- **Automation and project-graph diagnostics expose safer integrity signals.** Gateway routes now support dry-run checks and compact lifecycle audit output without leaking local paths.

## Fixes
- **Sensitive trace logs can be repaired locally.** A dedicated privacy repair tool redacts known sensitive values from historical turn traces and writes an auditable backup.
- **Waiting approval tasks no longer stay orphaned forever.** The integrity repair path can fail stale HITL waits that no longer have an active runtime owner.
- **Completed browser-budget tasks no longer pollute the runtime audit.** Historical tasks that finished after exhausting browser budget are reclassified so real-profile audits can return to zero hard errors.
- **The skill runtime no longer ships a vulnerable Wasmtime build.** Wasmtime is updated to the patched `46.0.3` release required by the RustSec gate.

## [0.1.1096] — 2026-08-30

A focused release candidate for browser PiP behavior and memory/privacy scope stability.

## Highlights
- **Browser controls now open the browser directly.** Clicking Browser in the working island opens the PiP browser instead of first expanding the right-side panel.
- **Memory briefing now follows the turn workspace.** Chat turns carry their resolved memory workspace explicitly, so project and personal recall do not drift through process-global workspace state.

## Improvements
- **Gateway prompt assembly uses explicit memory scope.** Briefing, recall payloads, relevant code context, project brief and recent work now share the same per-turn workspace identity.
- **Memory tests are more deterministic.** Gateway memory fixtures isolate their workspace state, reducing full-suite-only regressions that do not reproduce in a single test.

## Fixes
- **Browser rail clicks no longer require a second click for PiP.** The Browser section is no longer treated as a side-panel section by the workspace island state machine.
- **Project memory no longer leaks across concurrent gateway checks.** The in-process memory recall service treats the provided memory scope as authoritative instead of re-reading mutable global workspace context.

## [0.1.1095] — 2026-08-26

A release candidate focused on making the agent kernel easier to reason about, test and ship.

## Highlights
- **The chat runtime now has one typed turn contract.** Goal, policy, perimeter, recall, tools, execution identity, tracing and tail state now move through explicit owners instead of scattered scalar state.
- **The UI follows the canonical runtime projection.** Active turn, submission state, task queue, composer mode and thread status now render from the runtime view model instead of independent fallbacks.
- **Release readiness is stricter.** The pre-release and kernel gates now cover canonical turn lifecycle, gateway ownership, browser semantic progress and packaged-app smoke behavior before a release can be cut.

## Improvements
- **Legacy fallback paths have been removed.** Mock transcripts, preview seeds, starter-message helpers, local-ready copy and unused mock runtime exports no longer compete with real task state.
- **Planning and browser flow are easier to audit.** RC status evidence now records where planning, browser semantics and terminal task state come from, so regressions are checked against owners rather than screenshots.
- **Resource reservations no longer self-block a task.** A running turn can reuse its own browser reservation without being incorrectly held in `waiting_resource`.

## Fixes
- **Requests for an already-past time fail fast.** A same-day request such as “today at 8” is completed canonically with a visible explanation instead of launching browser automation.
- **Preflight completions close the whole turn.** The assistant message, terminal task state and terminal event are written together, so the UI no longer keeps showing a task as still thinking after a preflight answer.
- **Browser smoke tests reject cosmetic success.** A browser task must now produce semantic evidence from the page; generic “completed” states or unavailable-browser fallbacks fail the release gate.

## [0.1.1094] — 2026-07-30

A release candidate built around deterministic recovery, security gates and reproducible installers.

## Highlights
- **Hard restarts preserve one canonical turn.** Process fencing, durable checkpoints and lease recovery now converge tasks, runs and assistant messages without duplicate ownership after a gateway crash.
- **Release builds fail closed.** GitHub produces installers only after formatting, warning-free Clippy, the complete deterministic test gate and dependency audits all pass on the same source commit.

## Improvements
- **Every installer carries a SHA-256 manifest.** macOS, Windows and Linux artifacts can be verified independently before a draft release is considered for publication.
- **The inference surface matches the runtime.** The unused MistralRS transport and its unmaintained dependency tree have been removed; supported providers continue through the common inference contract.

## Fixes
- **Transient SQLite contention no longer loses a turn.** Atomic enqueue retries use fresh transactions, while startup recovery completes before any background database writer begins.
- **Rendered document QA starts reliably in CI.** Each Chromium run uses an isolated profile, a bounded DevTools readiness contract and complete process cleanup.

## [0.1.1093] — 2026-07-26

A browser that completes real tasks, and a noticeably smoother app.

## Highlights
- **Web searches now run to the end.** Ask for something that needs the web — train times, flights, a booking form — and Homun fills the form, waits for the results and reports the real values. Multi-field searches used to stop halfway and come back as a timeout.
- **Mid-task steering.** Correct or redirect Homun while it's already working, without starting over — your message is understood and applied to the task in progress.
- **The app is much smoother.** Replies scroll without bouncing as they stream, text updates without jank even on long answers, code blocks no longer flicker, and the window opens already in the correct theme.
- **Money actions require explicit confirmation.** Logins, bookings and form-filling stay free when you ask for them; only the final payment needs an explicit go-ahead, decided by what the action actually does on the page — never by the button's wording.

## Improvements
- **Dates and times are set in one step.** For ticket-style searches the browser sets a date or a time directly instead of clicking through a calendar, so those searches reach the results reliably.
- **Suggestion fields are handled properly.** Typing a station or city and picking the right suggestion now works first time, instead of re-typing the same thing over and over.
- **Homun keeps working while it is making progress.** Its limits now measure progress rather than elapsed time, so a long task is only stopped when it is genuinely stuck — and when it repeats itself it is told to change approach instead of giving up.
- **Homun keeps writing while the window is covered.** Previously, with the app in the background, the reply would freeze and then jump ahead when you switched back.
- **Faster startup.** Secondary views load only when they are needed.

## Fixes
- **Long results pages are read in full.** A results table was being cut off before its rows, so Homun could report "nothing found" while the results were on screen.
- **Search results are read once they have loaded.** After submitting a search it waits for the page to finish fetching instead of looking too early and starting over.
- **Paying online can complete.** Filling the card's security code was consuming the payment approval, so the final payment click was always refused.
- **A finished answer is never left spinning.** A turn could keep the "writing" indicator running after it had already answered, and could show the answer twice. The answer is now always delivered and the turn closed.
- **One unclear instruction no longer breaks the rest of the conversation.** An instruction sent mid-task that could not be interpreted used to leave every later message waiting.
- **Ordinary actions are no longer blocked by internal safety checks.** Everyday clicks, and commands such as deleting a build folder, were being refused; refusals now explain what to do, and only genuinely risky actions are stopped.
- **File paths are no longer mistaken for secrets.** Asking about a file whose name contains "secret" or "token" no longer hides it from Homun.
- **Long waits are no longer mistaken for errors.** If the model stays unavailable for a while, a wait stays a wait.
- **The local model respects its timeout.** A stuck generation no longer keeps Homun busy indefinitely.

[Unreleased]: https://github.com/homun-app/homun-releases/releases
[0.1.1099]: https://github.com/homun-app/homun-releases/releases/tag/v0.1.1099
[0.1.1098]: https://github.com/homun-app/homun-releases/releases/tag/v0.1.1098
[0.1.1096]: https://github.com/homun-app/homun-releases/releases/tag/v0.1.1096
[0.1.1095]: https://github.com/homun-app/homun-releases/releases/tag/v0.1.1095
[0.1.1094]: https://github.com/homun-app/homun-releases/releases/tag/v0.1.1094
[0.1.1093]: https://github.com/homun-app/homun-releases/releases/tag/v0.1.1093
