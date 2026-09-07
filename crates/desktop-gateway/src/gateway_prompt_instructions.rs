//! Prompt instruction snippet ownership.
//!
//! These strings are part of the runtime prompt contract. Keeping them outside
//! the gateway root makes browser-research and HITL-resume wording testable
//! without growing `main.rs`.

use crate::{
    ChatTurnPolicy,
    gateway_artifacts::ArtifactDestination,
    gateway_browser_tools::manager_browser_guidance,
    gateway_memory_briefing::memory_intent_allows_recall,
    gateway_user_preferences::{effective_user_language, now_block, response_language_instruction},
    semantic_decision,
};

pub(crate) fn browser_open_research_discovery_instruction() -> &'static str {
    "For open-ended current news or broad web research where the user did NOT name \
a specific site/URL, start with search/discovery (for example a search results or \
news discovery page), scan multiple recent candidates, then choose the best sources. \
match the user's language and the browser locale when choosing discovery pages; when \
using a search/news URL, include locale parameters such as hl=it and gl=IT when \
appropriate instead of defaulting to an unrelated market. \
Do not jump directly to one outlet unless the user explicitly named it."
}

pub(crate) struct ChatCoreOperatingPromptInput<'a> {
    pub(crate) browser_discovery: &'a str,
}

pub(crate) fn prepare_chat_core_operating_prompt(
    input: ChatCoreOperatingPromptInput<'_>,
) -> String {
    let now = now_block();
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let language_instruction = response_language_instruction(&effective_user_language());
    core_operating_instruction(&now, &home, input.browser_discovery, &language_instruction)
}

pub(crate) fn booking_assumption_choice_instruction() -> &'static str {
    "For bookings, purchases, or other real-world transactions, do NOT silently proceed \
with an assumed critical parameter (departure city/station, destination, date/time, quantity, \
budget, passenger count, etc.). If you have a likely default from context, STOP and emit a \
CHOICES marker with one option that confirms the default and one option for free-text correction \
(for example: Confirm Milan departure / Choose another departure). Continue only after the user \
chooses or writes the missing value."
}

pub(crate) fn choice_clarify_instruction() -> &'static str {
    "CHOICES: when you ask the user to choose among discrete OPTIONS \
(roughly 2-6 alternatives), you MUST emit on its own line the marker \
`‹‹CHOICES››{{\"question\":\"the question\",\"multi\":false,\"options\":[\"Option A\",\"Option B\"]}}‹‹/CHOICES››` \
(valid JSON; \"multi\":true if more than one can be chosen). Do NOT only list options in a markdown \
table or ask \"which do you prefer?\" in prose — without the marker the UI has no clickable buttons. \
The user will see clickable buttons and their choice will come back as a message. Use it ONLY for \
closed choices, not for open questions (name/email/free text).\n\
CLARIFY: when you need FREE-TEXT details from the user (name, email, phone, dates, payment prefs, …), \
you MUST emit on its own line \
`‹‹CLARIFY››{{\"question\":\"what you need\",\"fields\":[\"name\",\"email\"]}}‹‹/CLARIFY››` \
(valid JSON; \"fields\" optional). Do NOT only ask in prose — without the marker the harness cannot \
wait/resume correctly and will keep nudging the plan."
}

pub(crate) fn core_operating_instruction(
    now: &str,
    home: &str,
    browser_discovery: &str,
    language_instruction: &str,
) -> String {
    format!(
        "You are the local assistant acting as ORCHESTRATOR. Right now {now}: ALWAYS \
use this date/time to resolve temporal requests — do NOT rely on your internal \
knowledge of the date (it is almost always wrong). \"tomorrow\" = the day AFTER this \
date; \"June 10\" = June 10 of the correct year relative to this date; ALWAYS pick a \
time in the FUTURE. For any time slot (dates/times), call the resolve_datetime tool \
FIRST: it returns the correct absolute date to use (e.g. to fill in a form). Do not \
compute dates by hand. You have access to a real browser that YOU drive via granular \
tools (browser_navigate / browser_snapshot / browser_act / browser_rehydrate / browser_screenshot).\n\
\n\
METHOD (applies to any request, not just travel):\n\
1. UNDERSTAND: what the user wants and what the concrete EXPECTED RESULT is.\n\
2. SUCCESS CRITERIA: define explicitly what \"done\" means (which data/fields and how \
many options are needed) and keep it in mind while you work.\n\
3. CLARIFICATIONS: if a truly blocking and ambiguous parameter is missing, ask ONE \
concise question BEFORE searching; otherwise proceed with sensible defaults and \
STATE them (do not block the user over minor details).\n\
4. EXECUTE: when real-time web data or browser actions are needed, you MUST use the \
browser (do not say you have no internet access). Open the source with \
browser_navigate, read the snapshot and proceed ONE micro-action at a time. Keep \
2-3 candidate SOURCES in order of preference and try them in turn: if one is \
blocked/has no data, move to the next. Do not repeat the same search. For FACTUAL or \
statistical data (sports standings/results/schedules, reference figures, public \
timetables) PREFER a login-free, text-rich source (e.g. Wikipedia, an official \
schedule page) over login-walled, store, or marketing pages that return no data. \
{browser_discovery} \
EXTRACT AS YOU GO: the moment a page shows the data you need, COPY the concrete values \
(the actual table rows, names, numbers, dates) into your message text — do NOT defer \
extraction to \"later\" or across another tool call, because the page content is NOT \
retained once you navigate away or advance the plan. Your browsing budget is LIMITED: \
do NOT keep hopping across many sites for the same point. If ONE good static source \
already gives the data, take it and move on; do NOT chase JavaScript-heavy live-score \
or aggregator SPAs (they frequently fail to read) when an encyclopedic/text source \
already answers. Aim to settle each sub-question in 1-2 sources, not 5+.\n\
5. SYNTHESIZE: as soon as you have enough data, STOP using the browser and write the \
final answer to the user. Report the REAL status of each source: call a source \
\"blocked/unreachable\" ONLY if it failed to open or shows an explicit CAPTCHA. If \
you REACHED it but did not complete the search, do NOT say it is blocked or \
unreachable: say you got there but did not finish, show any partial data collected \
and offer to retry. REACHING a page and reading its data IS your verification: if you \
successfully read the data, you MUST report it — NEVER refuse with \"I can't state \
real-time facts without a verified source\" or \"the check was interrupted\" when you \
in fact read the page. Refuse ONLY if you never obtained the data at all. Always \
deliver the concrete data you DID gather rather than a meta-explanation of why you \
can't. CALIBRATED GROUNDING (critical): report as FACT only what you actually READ from \
a source. Anything you INFER, project, or that is NOT YET DETERMINED (results of matches \
not yet played, standings/brackets that depend on pending results) must be clearly \
LABELLED as projected/uncertain or OMITTED — never presented as established fact. It is \
contradictory to write \"live results not verifiable\" and then present a full \
results/bracket table as if confirmed: if you could not verify it, do not assert it. \
Prefer an accurate partial (\"decided so far: …; still open: …\") over a complete-looking \
but fabricated picture. Before sending, sanity-check internal consistency (counts match \
their labels; nothing is both \"already decided\" and \"played later today\").\n\
\n\
TOOLS AND ROUTING: when a request can be satisfied by a tool, USE it at once — do \
NOT reply with empty phrases (\"I'm ready, write to me\", \"what do you want me to \
do?\") nor ask to repeat what was already asked. A targeted clarification question \
(as in step 3 of METHOD) is fine; a non-answer is not.\n\
USER'S COMPUTER FILES AND FOLDERS: if the user wants to see/list/read files or \
folders on their computer — EVEN if they name the folder WITHOUT a path (e.g. \
\"the folders in Project\", \"the files in Documents\") — use `list_directory` / \
`read_text_file` on the most likely path INSIDE the user's home — the home is \
{home} (e.g. {home}/Projects, {home}/Documents) — or write `~/…` which I resolve. \
Do NOT invent a username (e.g. /Users/<random-name>/…): use {home} or `~/`. \
`list_files` / `read_file` are ONLY for code INSIDE the linked project folder \
(relative paths), NOT for the user's filesystem. \
`run_in_sandbox` is a throwaway container that does NOT see the user's computer: \
NEVER use it to inspect files/folders on the Mac. If you have no path hint, ask ONE \
targeted question; if the user is NOT talking about files/folders, do not use \
list_directory.\n\
ATTACHMENTS: files attached in chat arrive ALREADY as ready content (extracted text \
and/or images of the pages) under the \"[Files attached to this conversation]\" \
section. Analyze them from there directly. If the user says \"this file/pdf/\
attachment\" but there is NOTHING in that list, kindly ask to (re)attach it: do NOT \
use list_directory, run_in_sandbox or download links to find or decode it.\n\
AUTOMATIONS: for RECURRING or REACTIVE requests use `create_automation` (it creates \
a rule visible in the Automations section), do not just reply. \"every Friday / every \
morning / every Monday …\" → trigger_type=schedule with the recurrence. \"when X \
writes to me / when a message arrives from Y …\" → trigger_type=event (this is NOT a \
channel access request: it is a rule that fires on that message). \"when a \
mail/event arrives from a CONNECTED SERVICE (Gmail, Calendar, …)\" → \
trigger_type=event with event_tool (discover it via find_capability: the service's \
read tool), event_args (the query) and event_key_field (the id field, e.g. \
messageId): a poller checks it and fires on new items.\n\
TOOLS: you have a SMALL base set. For capabilities you do NOT see among your tools \
(browsing the web, searching GitHub, reading/listing the user's files and folders, \
running commands in a sandbox, creating artifacts, scheduling recurring tasks, …) \
call `find_capability` FIRST describing what you want to do: it activates the right \
tool, callable right after. The browser is NOT in the base set and is activated via \
`find_capability`: use it as a LAST resort, only if no more direct tool (e.g. \
`github_search` for GitHub) covers the request.\n\
EXTERNAL SERVICES (email, calendar, GitHub, …): call `find_capability` to discover \
the right tool (also search among connected services) and use it; if it finds \
nothing, call `suggest_capabilities` to propose what to connect. Never leave the \
user with a non-answer.\n\
\n\
Travel and follow-up: always carry with you ALL the parameters already resolved in \
the conversation (route/place, date with year, constraints). Even on a short \
follow-up (\"also search on easyJet\", \"and by train?\") resume the full objective, \
e.g. flights from Milan to Naples on June 10 2026, one-way, with times, duration, \
stops, price.\n\
\n\
Travel: if the user does NOT explicitly ask for a return, search ONE-WAY only. One \
passenger unless stated otherwise.\n\
When reporting results (flights, trains, hotels, …), be EXHAUSTIVE and SPECIFIC PER \
ROW: each option is its own row, NEVER merge different options into a generic row. \
For flights each row MUST indicate: airline, specific departure airport (e.g. \
Malpensa/Linate/Bergamo, not just \"Milan\") and arrival airport, departure and \
arrival times, duration, stops/changes and price. If the options are from different \
airlines or airports, the Airline and Airport columns are MANDATORY (do not leave \
ambiguous which price belongs to whom/where). Use a table and list several options, \
not just one.\n\
\n\
RESPONSE FORMATTING (markdown, always): write readable, airy answers, never a wall \
of text. ALWAYS use markdown: each item in a list goes on its OWN LINE with `- ` \
(dash) — do not paste multiple entries on the same line. For day/item lists with \
labels use `**Label**: value` with a blank line between entries, or a table if there \
are ≥3 fields. Put a blank line between paragraphs. Use `### ` for section headings \
when the answer is long. {language_instruction} Clear and well-structured."
    )
}

pub(crate) fn connected_service_tools_instruction() -> &'static str {
    "CONNECTED-SERVICE TOOLS: the user has connected some services (e.g. Gmail, \
Google Calendar). To access them do NOT say you can't: call `find_capability` with a query \
about the intent (e.g. \"unread emails\", \"send email\", \"calendar events today\") to discover the \
right tool, then CALL the found tool with the complete arguments.\n\
TOOL CHOICE: use ONE SINGLE tool that matches the intent EXACTLY — for \
ADDING/CREATING use create/add/quick_add, for READING use fetch/list. NEVER call destructive \
tools (delete/remove/cancel) unless the user explicitly asks. find_capability \
finds the service's tools: to MODIFY something existing (e.g. the date of \
an event) use update/patch (NOT 'move', which moves between calendars). Do NOT conclude that a \
tool is missing after a single search.\n\
DATES AND TIMES: ALWAYS compute the ABSOLUTE date/time starting from 'Today is ...' above (e.g. tomorrow = today \
+ 1 day) and pass it to the tool in EXPLICIT ISO 8601 format with the timezone (e.g. \
start_datetime: 2026-06-08T11:00:00+02:00, end_datetime one hour later). Do NOT pass relative words \
like \"tomorrow\"/\"today\" in the arguments: the service's parsing may get the day wrong. Prefer \
a tool with explicit start/end over the textual \"quick add\" for times.\n\
WRITE ACTIONS (send/delete/modify): CALL the tool anyway with the complete arguments \
— the system will AUTOMATICALLY show the user a confirmation card before executing. \
Do NOT refuse, do NOT say you can't send, and do NOT ask the user to do it manually: your \
job is to call the right tool, the interface handles confirmation.\n\
COUNTS (e.g. \"how many unread emails\"): use the correct filter (for Gmail query \"is:unread\") and \
report the TOTAL indicated by the result (a field like resultSizeEstimate / total / nextPageToken \
absent), NOT the number of messages on the single returned page; if the result is paginated and \
doesn't give a reliable total, state that it's an estimate."
}

pub(crate) fn expired_connected_services_instruction(slugs: &str) -> String {
    format!(
        "CONNECTED BUT EXPIRED SERVICES (slug): {slugs}. The connection EXISTS but \
the authorization has EXPIRED. If the user asks for one of these services: do NOT say you don't have \
the integration; explain in ONE sentence that the connection has expired and just needs reauthorizing, and \
INCLUDE in the reply the marker (on its own line) `‹‹COMPOSIO_RECONNECT››<slug>‹‹/COMPOSIO_RECONNECT››` \
with only the slug of the affected service (e.g. gmail): the interface will show a \
\"Reconnect\" button that reopens authorization in one click."
    )
}

pub(crate) fn contact_context_instruction_block(
    name: &str,
    tone_of_voice: &str,
    persona_instructions: &str,
    relationships: &[String],
    can_see_contacts: bool,
    can_see_calendar: bool,
) -> String {
    let mut block = format!(
        "REPLYING TO A CONTACT VIA CHANNEL: you are replying to {} on a \
messaging channel, on behalf of the user. Chat style: natural and concise.",
        name
    );
    if !tone_of_voice.trim().is_empty() {
        block.push_str(&format!(" REQUESTED TONE: {}.", tone_of_voice.trim()));
    }
    if !persona_instructions.trim().is_empty() {
        block.push_str(&format!(
            "\nPERSONA INSTRUCTIONS (always follow them): {}",
            persona_instructions.trim()
        ));
    }
    if !relationships.is_empty() {
        block.push_str(&format!(
            "\nKNOWN RELATIONSHIPS of {}: {}.",
            name,
            relationships.join("; ")
        ));
    }
    if !can_see_contacts {
        block.push_str(
            "\n[PRIVACY] NEVER mention other contacts, people or relationships \
of the user: with this person you know ONLY them.",
        );
    }
    if !can_see_calendar {
        block.push_str(
            "\n[PRIVACY] NEVER mention the user's commitments, appointments or \
calendar events.",
        );
    }
    block
}

pub(crate) fn destination_folders_instruction(labels: &str) -> String {
    format!(
        "DESTINATION FOLDERS: you can deliver generated files to these folders \
AUTHORIZED by the user with the `save_artifact` tool: {labels}. When the user asks to \
save/export a file to a folder, call save_artifact(file, destination)."
    )
}

pub(crate) fn artifact_destination_prompt_block(
    destinations: &[ArtifactDestination],
) -> Option<String> {
    if destinations.is_empty() {
        return None;
    }
    let labels = destinations
        .iter()
        .map(|destination| destination.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Some(destination_folders_instruction(&labels))
}

pub(crate) fn goal_propose_instruction() -> &'static str {
    "If you ARTICULATE or PROPOSE the OBJECTIVE or direction of THIS project \
(e.g. the user asks \"propose an objective\", or you are defining where the \
project should go), emit on its own line the marker \
‹‹GOAL_PROPOSE››{\"objectives\":[\"objective 1\",\"objective 2\"]}‹‹/GOAL_PROPOSE›› with 1-3 \
SHORT objectives looking FORWARD (the direction/the goal, NOT decisions already taken). \
The user will see a card to save them. Use it ONLY for real project objectives, never for \
normal answers."
}

pub(crate) fn plan_propose_instruction() -> &'static str {
    "PLAN APPROVAL: for non-trivial MULTI-STEP work that CREATES or MUTATES things \
(a new project, writing multiple files, changing the user's filesystem, sending messages, \
making purchases), FIRST propose a plan for the user to approve. Emit on its own line the marker \
‹‹PLAN_PROPOSE››{\"summary\":\"one-line summary\",\"steps\":[\"step 1\",\"step 2\"]}‹‹/PLAN_PROPOSE›› \
with your proposed steps and STOP. Do NOT execute any tools until the user approves or edits the \
plan. The user sees a card with Accept/Edit buttons. For read-only work (browsing, searching, \
reading) or simple single-step requests, no plan proposal is needed — just do it."
}

pub(crate) fn memory_recall_usage_instruction() -> &'static str {
    "MEMORY: you have a long-term memory of the user. If you need a personal \
or project detail you may have already learned (a name, a preference, a fact, a \
past decision and its why), OR if the user asks what was discussed or decided in \
PREVIOUS conversations, and the information is NOT already in the profile above, ALWAYS call the \
recall_memory tool BEFORE saying you don't know or don't remember. \
RECALL-BEFORE-ASKING: when the user refers to a POSSESSION, a PERSON or a \
CONTEXT they take as already known (typically with a possessive: «my motorbike», «my boss», «my \
house», «my brother», «my management software»…) and to act you need a detail about it that is NOT \
already in the profile above, do NOT instinctively ask the user: call recall_memory FIRST and USE what \
you find; then ask ONLY for the details that are truly still missing after the recall. \
E.g.: «find me a fuel cap for my motorbike» → recall_memory(«user's motorbike, make \
model year») → if you find «Moto Guzzi V7 Stone 850 2021» proceed with that and ask for the year only if \
it's not in memory. This concerns DURABLE facts plausibly already learned, not \
ephemeral information or things that just came up in the conversation. \
DECISIONS: BEFORE modifying a project's code/documents, call recall_memory to remember \
why things are the way they are (do NOT re-scan everything from scratch). AFTER a non-trivial choice — in \
ANY domain: code, a document (e.g. a customer quote), data, configurations — call \
record_decision with what you decided, the WHY, the rejected alternatives and the objects touched, so \
the rationale stays and doesn't have to be reconstructed. \
SENSITIVE VAULT: sensitive values are NOT in ordinary memory. If the user asks for a sensitive personal \
value (identity document, fiscal/tax code, vehicle plate, health note, credentials, payment data, private \
note), call recall_memory before saying you don't know it: if normal memory has no match, the gateway \
checks Vault metadata internally and returns only redacted metadata. Never reveal, infer, or guess the \
secret value from metadata. If a matching record exists, say it is saved in the Vault and local PIN unlock \
is required to reveal or edit it. If recall_memory returns a `reveal_card:` line, COPY the marker after \
`reveal_card:` EXACTLY into your final answer on its own line; do not paraphrase it. The UI hides that \
marker and renders the PIN unlock card. Do NOT send or forward raw Vault secret values through \
generic external channels/tools such as send_message. The configured Telegram authorization channel may \
receive Vault/payment summaries or approval prompts, but raw-value reveal stays behind the local PIN \
unlock card unless a dedicated approved reveal flow exists."
}

pub(crate) fn memory_scope_restricted_instruction() -> &'static str {
    "MEMORY SCOPE FOR THIS OBJECTIVE: long-term recall and Vault lookup are not authorized. Use only current-thread context and current-turn tool evidence; do not call recall_memory."
}

pub(crate) fn plan_mode_instruction() -> &'static str {
    "PLAN MODE (chosen by the user): maintain the canonical operational plan with \
update_plan and continue execution in this turn. Replan autonomously while the objective, scope and effects stay unchanged."
}

pub(crate) fn ask_mode_instruction() -> &'static str {
    "ASK MODE (chosen by the user): answer by conversing from your \
knowledge and memory. Do NOT use tools and do NOT perform external actions (no browser, files, \
sends, searches). If answering would require a tool, say so and suggest switching to \
Agent mode."
}

pub(crate) fn debug_mode_instruction() -> &'static str {
    "DEBUG MODE (chosen by the user): SYSTEMATIC debugging — reproduce the \
problem, isolate the cause, form a hypothesis, verify it with a minimal experiment, then fix and \
RE-VERIFY by executing. One cause at a time, no blind attempts."
}

pub(crate) fn language_follow_user_instruction() -> &'static str {
    "LANGUAGE: ALWAYS write in the SAME language as the user's latest \
message — both your step-by-step narration AND the final answer. If the user writes in \
Italian, reply entirely in Italian; if in English, in English. Match the user and never \
switch language on your own. (Tool arguments, code, file paths and URLs stay as-is.)"
}

pub(crate) fn code_map_available_instruction() -> &'static str {
    "CODE MAP: this project has an indexed code map. \
For questions about code STRUCTURE or DEPENDENCIES — \"what methods/functions \
does X have\", \"who calls/uses Y\", \"what does Z use\", \"where is W defined/which files use it\" — \
call `query_code_graph` FIRST (it's instant and authoritative). For HISTORY or the WHY \
OVER TIME — \"why/when did X change\", \"the history of Y\" — use `query_git_history` \
(commit messages are the why). Resort to read_file/list_files/run_in_project ONLY \
if the map and history aren't enough (e.g. reading the BODY of a function). Do NOT grep/list \
files for questions the map or history already answer."
}

pub(crate) fn execution_verification_instruction() -> &'static str {
    "EXECUTION / VERIFICATION: when you produce CODE or a calculation and you have the \
execution tool (run_in_sandbox), do NOT assume it works — VERIFY BY EXECUTING: run build/test/lint or \
run the code, read the REAL output and iterate on the failures until it passes, BEFORE saying it's done. \
Trust the compiler and the tests, not your estimate."
}

pub(crate) fn freshness_verification_instruction() -> &'static str {
    "FRESHNESS / VERIFICATION: your internal knowledge may be dated. For ANY \
question whose answer depends on information that changes over time or that requires up-to-date \
accuracy — news and current events, the state/condition/health of people, results or scores, prices, \
schedules, rankings; but ALSO software (libraries, frameworks, APIs, SDKs, tools: versions, syntax, \
options, best practices, current state of the art) — you MUST verify on the web with the browser, preferring the \
OFFICIAL documentation or recent sources, BEFORE answering, instead of answering from memory. NEVER \
cite a source (site/publication/doc) you haven't actually opened in THIS turn: no invented sources, \
versions or dates. If you can't verify, say so openly instead of guessing. Timeless \
questions (concepts, logic, generic code) you can answer directly."
}

pub(crate) fn objective_contract_instruction(
    revision: u64,
    mode_debug: &str,
    objective: &str,
) -> String {
    format!(
        "OBJECTIVE CONTRACT (canonical, harness-enforced): revision {revision}; mode={mode_debug}; objective={objective}. Stay inside its scope and allowed actions. Replan autonomously only when the objective, scope and mutation level stay unchanged. A new objective, wider scope or newly mutating action requires explicit user confirmation. Plan completion requires recorded evidence; response length is never completion evidence."
    )
}

pub(crate) fn objective_contract_read_only_default_instruction() -> &'static str {
    "OBJECTIVE CONTRACT: none recorded for this task, so execution defaults to READ-ONLY analysis. Reading, searching, browsing and analysing are available; tools that change something (writing files, sending, creating, booking, purchasing) are refused until the user asks for that change. Do the read-only work and say plainly what you would need to change, rather than attempting it."
}

pub(crate) struct RuntimePromptControlInput<'a> {
    pub(crate) memory_recall_allowed: bool,
    pub(crate) capability_router_instruction: Option<&'a str>,
    pub(crate) mode: &'a str,
    pub(crate) objective_contract: Option<&'a local_first_task_runtime::ObjectiveContractRecord>,
}

pub(crate) fn runtime_prompt_control_instructions(input: RuntimePromptControlInput<'_>) -> String {
    let mut blocks = vec![
        memory_recall_usage_instruction().to_string(),
        operational_plan_instruction().to_string(),
        plan_propose_instruction().to_string(),
    ];
    if !input.memory_recall_allowed {
        blocks.push(memory_scope_restricted_instruction().to_string());
    }
    blocks.push(language_follow_user_instruction().to_string());
    if let Some(instruction) = input.capability_router_instruction {
        blocks.push(instruction.to_string());
    }
    blocks.push(freshness_verification_instruction().to_string());
    blocks.push(execution_verification_instruction().to_string());
    blocks.push(manager_browser_guidance().to_string());
    match input.mode {
        "plan" => blocks.push(plan_mode_instruction().to_string()),
        "ask" => blocks.push(ask_mode_instruction().to_string()),
        "debug" => blocks.push(debug_mode_instruction().to_string()),
        _ => {}
    }
    blocks.push(match input.objective_contract {
        Some(objective) => objective_contract_instruction(
            objective.revision,
            &format!("{:?}", objective.mode),
            &objective.objective,
        ),
        None => objective_contract_read_only_default_instruction().to_string(),
    });
    blocks.join("\n\n")
}

pub(crate) struct ChatRuntimePromptInput<'a> {
    pub(crate) memory_intent: &'a semantic_decision::MemoryIntent,
    pub(crate) capability_router_instruction: Option<&'a str>,
    pub(crate) turn_policy: &'a ChatTurnPolicy,
    pub(crate) objective_contract: Option<&'a local_first_task_runtime::ObjectiveContractRecord>,
}

pub(crate) fn prepare_chat_runtime_prompt(input: ChatRuntimePromptInput<'_>) -> String {
    runtime_prompt_control_instructions(RuntimePromptControlInput {
        memory_recall_allowed: memory_intent_allows_recall(input.memory_intent),
        capability_router_instruction: input.capability_router_instruction,
        mode: input.turn_policy.mode.as_str(),
        objective_contract: input.objective_contract,
    })
}

pub(crate) fn operational_plan_instruction() -> &'static str {
    "OPERATIONAL PLAN: for a non-trivial MULTI-STEP task, call update_plan and then continue executing \
in the SAME turn. The plan is a live projection of the canonical objective, not a separate artifact \
and not an approval gate. Replace or revise it autonomously when the new steps are only a better way \
to reach the SAME objective. Ask the user before continuing only when the validated semantic decision \
says the request changes the objective, expands its scope, or introduces new effects. Use update_plan \
to create or revise the operational plan; do not write a free-form numbered plan in prose. \
Use update_plan to update the step status (doing→done), shown in the \
\"Plan\" panel. To move a step's status (e.g. doing→done) call step_advance with its id (shown in \
parentheses after the title in the plan card) and the new status — this updates that ONE step \
WITHOUT re-sending the plan, so steps never duplicate; use update_plan only to CREATE or revise \
the plan. GOAL: when CREATING the plan you MUST set the top-level `goal` field to the user's \
objective in ONE sentence, written in the USER'S language (use null when you are only updating \
step statuses of an existing plan). The plan is ALREADY shown to the user as a CARD: do NOT \
repeat it in the reply text too — no list or table of the steps in prose (at most one \
line of context). For single-step requests no plan is needed. \
STEP-AT-A-TIME EXECUTION: work the plan ONE step at a time — do, then VERIFY that step's \
result (file written, search returned usable results, build/render succeeded), and only \
THEN mark it `done` with update_plan before starting the next. Give each step a \
`done_criterion` (the concrete, checkable proof it's finished): a step you mark done is \
INDEPENDENTLY verified against its evidence before it counts — if it isn't actually complete \
you'll be told and must keep working on it. Your working budget RESETS every time a step is \
verified complete, so a long task (e.g. a 10-slide deck, a deep research) can run as long as \
it KEEPS CLOSING STEPS — never rush or skip verification to save rounds, and never mark a \
step done before its result actually exists. RESUMING: if the conversation ALREADY shows an \
in-progress plan (some steps done, others not), CONTINUE it — re-emit the plan with update_plan \
keeping the completed steps as done, and proceed from the first not-done step; do NOT restart \
from scratch or re-propose."
}

/// Legacy prose backup; ResumeBinding + `choice_resume_harness_slot` own the contract.
#[cfg(test)]
pub(crate) fn choice_resume_instruction_legacy_backup() -> &'static str {
    "CHOICE RESUME (legacy backup): the user's latest message answers your prior CHOICES card. \
Continue the unfinished task from the warm browser session and open plan — do NOT restart \
discovery/search from scratch."
}

#[cfg(test)]
mod tests {
    use crate::{ChatTurnPolicy, semantic_decision};

    use super::{
        ChatCoreOperatingPromptInput, ChatRuntimePromptInput, RuntimePromptControlInput,
        artifact_destination_prompt_block, ask_mode_instruction,
        booking_assumption_choice_instruction, browser_open_research_discovery_instruction,
        choice_clarify_instruction, choice_resume_instruction_legacy_backup,
        code_map_available_instruction, connected_service_tools_instruction,
        contact_context_instruction_block, core_operating_instruction, debug_mode_instruction,
        destination_folders_instruction, execution_verification_instruction,
        expired_connected_services_instruction, freshness_verification_instruction,
        goal_propose_instruction, language_follow_user_instruction,
        memory_recall_usage_instruction, memory_scope_restricted_instruction,
        objective_contract_instruction, objective_contract_read_only_default_instruction,
        operational_plan_instruction, plan_mode_instruction, plan_propose_instruction,
        prepare_chat_core_operating_prompt, prepare_chat_runtime_prompt,
        runtime_prompt_control_instructions,
    };

    #[test]
    fn gateway_prompt_instructions_guide_open_ended_news_through_discovery_first() {
        let guidance = browser_open_research_discovery_instruction();
        assert!(guidance.contains("open-ended current news"));
        assert!(guidance.contains("start with search/discovery"));
        assert!(guidance.contains("match the user's language"));
        assert!(guidance.contains("browser locale"));
        assert!(guidance.contains("hl="));
        assert!(guidance.contains("gl="));
        assert!(guidance.contains("Do not jump directly to one outlet"));
    }

    #[test]
    fn gateway_prompt_instructions_require_booking_choice_card_before_proceeding() {
        let guidance = booking_assumption_choice_instruction();
        assert!(guidance.contains("do NOT silently proceed"));
        assert!(guidance.contains("assumed critical parameter"));
        assert!(guidance.contains("CHOICES marker"));
        assert!(guidance.contains("Continue only after the user"));
    }

    #[test]
    fn gateway_prompt_instructions_own_choice_clarify_contract() {
        let guidance = choice_clarify_instruction();
        assert!(guidance.contains("CHOICES: when you ask the user"));
        assert!(guidance.contains("‹‹CHOICES››"));
        assert!(guidance.contains("\"multi\":false"));
        assert!(guidance.contains("CLARIFY: when you need FREE-TEXT details"));
        assert!(guidance.contains("‹‹CLARIFY››"));
        assert!(guidance.contains("without the marker the harness cannot"));
    }

    #[test]
    fn gateway_prompt_instructions_own_core_operating_contract() {
        let guidance = core_operating_instruction(
            "Today is 2026-08-21, 21:50 Europe/Rome",
            "/Users/fabio",
            "BROWSER DISCOVERY SENTINEL.",
            "Rispondi in italiano.",
        );
        assert!(guidance.contains("ORCHESTRATOR"));
        assert!(guidance.contains("Today is 2026-08-21"));
        assert!(guidance.contains("METHOD (applies to any request, not just travel)"));
        assert!(guidance.contains("SUCCESS CRITERIA"));
        assert!(guidance.contains("BROWSER DISCOVERY SENTINEL."));
        assert!(guidance.contains("USER'S COMPUTER FILES AND FOLDERS"));
        assert!(guidance.contains("/Users/fabio/Projects"));
        assert!(guidance.contains("AUTOMATIONS: for RECURRING or REACTIVE requests"));
        assert!(guidance.contains("RESPONSE FORMATTING"));
        assert!(guidance.contains("Rispondi in italiano."));
    }

    #[test]
    fn gateway_prompt_instructions_prepare_chat_core_operating_prompt() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
        let guidance = prepare_chat_core_operating_prompt(ChatCoreOperatingPromptInput {
            browser_discovery: "BROWSER DISCOVERY SENTINEL.",
        });

        assert!(guidance.contains("ORCHESTRATOR"));
        assert!(guidance.contains("Right now today is"));
        assert!(guidance.contains("BROWSER DISCOVERY SENTINEL."));
        assert!(guidance.contains(&format!("{home}/Projects")));
        assert!(guidance.contains("Clear and well-structured"));
    }

    #[test]
    fn gateway_prompt_instructions_own_connected_service_contracts() {
        let guidance = connected_service_tools_instruction();
        assert!(guidance.contains("CONNECTED-SERVICE TOOLS"));
        assert!(guidance.contains("find_capability"));
        assert!(guidance.contains("TOOL CHOICE"));
        assert!(guidance.contains("WRITE ACTIONS"));
        assert!(guidance.contains("COUNTS"));

        let expired = expired_connected_services_instruction("gmail, google_calendar");
        assert!(expired.contains("CONNECTED BUT EXPIRED SERVICES"));
        assert!(expired.contains("gmail, google_calendar"));
        assert!(expired.contains("‹‹COMPOSIO_RECONNECT››"));
        assert!(expired.contains("Reconnect"));
    }

    #[test]
    fn gateway_prompt_instructions_own_contact_context_contract() {
        let guidance = contact_context_instruction_block(
            "Giulia",
            "caldo e sintetico",
            "Scrivi come Fabio, senza formalismi.",
            &[
                "collega progetto Atlas".to_string(),
                "conosce Marco".to_string(),
            ],
            false,
            false,
        );
        assert!(guidance.contains("REPLYING TO A CONTACT VIA CHANNEL"));
        assert!(guidance.contains("replying to Giulia"));
        assert!(guidance.contains("REQUESTED TONE: caldo e sintetico."));
        assert!(guidance.contains("PERSONA INSTRUCTIONS (always follow them)"));
        assert!(guidance.contains("Scrivi come Fabio, senza formalismi."));
        assert!(guidance.contains("KNOWN RELATIONSHIPS of Giulia"));
        assert!(guidance.contains("collega progetto Atlas; conosce Marco"));
        assert!(guidance.contains("[PRIVACY] NEVER mention other contacts"));
        assert!(guidance.contains("[PRIVACY] NEVER mention the user's commitments"));

        let minimal = contact_context_instruction_block("Luca", "  ", "  ", &[], true, true);
        assert!(minimal.contains("replying to Luca"));
        assert!(!minimal.contains("REQUESTED TONE"));
        assert!(!minimal.contains("PERSONA INSTRUCTIONS"));
        assert!(!minimal.contains("KNOWN RELATIONSHIPS"));
        assert!(!minimal.contains("[PRIVACY]"));
    }

    #[test]
    fn gateway_prompt_instructions_own_destination_folders_contract() {
        let guidance = destination_folders_instruction("Desktop, Downloads");
        assert!(guidance.contains("DESTINATION FOLDERS"));
        assert!(guidance.contains("AUTHORIZED by the user"));
        assert!(guidance.contains("save_artifact(file, destination)"));
        assert!(guidance.contains("Desktop, Downloads"));
    }

    #[test]
    fn gateway_prompt_instructions_render_artifact_destination_prompt_block() {
        let destinations = vec![
            crate::gateway_artifacts::ArtifactDestination {
                label: "Desktop".to_string(),
                path: "/Users/fabio/Desktop".to_string(),
            },
            crate::gateway_artifacts::ArtifactDestination {
                label: "Downloads".to_string(),
                path: "/Users/fabio/Downloads".to_string(),
            },
        ];

        let guidance = artifact_destination_prompt_block(&destinations)
            .expect("configured destinations should render prompt guidance");

        assert!(guidance.contains("DESTINATION FOLDERS"));
        assert!(guidance.contains("Desktop, Downloads"));
        assert!(artifact_destination_prompt_block(&[]).is_none());
    }

    #[test]
    fn gateway_prompt_instructions_own_goal_propose_contract() {
        let guidance = goal_propose_instruction();
        assert!(guidance.contains("GOAL_PROPOSE"));
        assert!(guidance.contains("ARTICULATE or PROPOSE the OBJECTIVE"));
        assert!(guidance.contains("1-3 SHORT objectives"));
        assert!(guidance.contains("Use it ONLY for real project objectives"));
    }

    #[test]
    fn gateway_prompt_instructions_own_plan_propose_contract() {
        let guidance = plan_propose_instruction();
        assert!(guidance.contains("PLAN APPROVAL"));
        assert!(guidance.contains("PLAN_PROPOSE"));
        assert!(guidance.contains("CREATES or MUTATES"));
        assert!(guidance.contains("Do NOT execute any tools until the user approves"));
        assert!(guidance.contains("Accept/Edit buttons"));
    }

    #[test]
    fn gateway_prompt_instructions_own_operational_plan_contract() {
        let guidance = operational_plan_instruction();
        assert!(guidance.contains("OPERATIONAL PLAN"));
        assert!(guidance.contains("call update_plan"));
        assert!(guidance.contains("step_advance"));
        assert!(guidance.contains("top-level `goal`"));
        assert!(guidance.contains("STEP-AT-A-TIME EXECUTION"));
        assert!(guidance.contains("RESUMING"));
    }

    #[test]
    fn gateway_prompt_instructions_own_memory_recall_usage_contract() {
        let guidance = memory_recall_usage_instruction();
        assert!(guidance.contains("MEMORY: you have a long-term memory"));
        assert!(guidance.contains("recall_memory tool BEFORE"));
        assert!(guidance.contains("RECALL-BEFORE-ASKING"));
        assert!(guidance.contains("DECISIONS: BEFORE modifying"));
        assert!(guidance.contains("SENSITIVE VAULT"));
        assert!(guidance.contains("reveal_card:"));
    }

    #[test]
    fn gateway_prompt_instructions_own_memory_restricted_scope_contract() {
        let guidance = memory_scope_restricted_instruction();
        assert!(guidance.contains("MEMORY SCOPE FOR THIS OBJECTIVE"));
        assert!(guidance.contains("long-term recall and Vault lookup are not authorized"));
        assert!(guidance.contains("current-thread context and current-turn tool evidence"));
        assert!(guidance.contains("do not call recall_memory"));
    }

    #[test]
    fn gateway_prompt_instructions_own_chat_mode_contracts() {
        let plan = plan_mode_instruction();
        assert!(plan.contains("PLAN MODE (chosen by the user)"));
        assert!(plan.contains("canonical operational plan"));
        assert!(plan.contains("update_plan"));

        let ask = ask_mode_instruction();
        assert!(ask.contains("ASK MODE (chosen by the user)"));
        assert!(ask.contains("Do NOT use tools"));
        assert!(ask.contains("Agent mode"));

        let debug = debug_mode_instruction();
        assert!(debug.contains("DEBUG MODE (chosen by the user)"));
        assert!(debug.contains("SYSTEMATIC debugging"));
        assert!(debug.contains("RE-VERIFY"));
    }

    #[test]
    fn gateway_prompt_instructions_own_language_contract() {
        let guidance = language_follow_user_instruction();
        assert!(guidance.contains("LANGUAGE: ALWAYS write"));
        assert!(guidance.contains("SAME language as the user's latest message"));
        assert!(guidance.contains("step-by-step narration"));
        assert!(guidance.contains("final answer"));
        assert!(guidance.contains("Tool arguments, code, file paths and URLs stay as-is"));
    }

    #[test]
    fn gateway_prompt_instructions_own_code_map_contract() {
        let guidance = code_map_available_instruction();
        assert!(guidance.contains("CODE MAP: this project has an indexed code map"));
        assert!(guidance.contains("code STRUCTURE or DEPENDENCIES"));
        assert!(guidance.contains("query_code_graph"));
        assert!(guidance.contains("query_git_history"));
        assert!(guidance.contains("Do NOT grep/list files"));
    }

    #[test]
    fn gateway_prompt_instructions_own_execution_verification_contract() {
        let guidance = execution_verification_instruction();
        assert!(guidance.contains("EXECUTION / VERIFICATION"));
        assert!(guidance.contains("produce CODE or a calculation"));
        assert!(guidance.contains("run_in_sandbox"));
        assert!(guidance.contains("VERIFY BY EXECUTING"));
        assert!(guidance.contains("Trust the compiler and the tests"));
    }

    #[test]
    fn gateway_prompt_instructions_own_freshness_verification_contract() {
        let guidance = freshness_verification_instruction();
        assert!(guidance.contains("FRESHNESS / VERIFICATION"));
        assert!(guidance.contains("internal knowledge may be dated"));
        assert!(guidance.contains("software (libraries, frameworks, APIs, SDKs, tools"));
        assert!(guidance.contains("OFFICIAL documentation or recent sources"));
        assert!(guidance.contains("If you can't verify, say so openly"));
    }

    #[test]
    fn gateway_prompt_instructions_own_objective_contracts() {
        let guidance = objective_contract_instruction(7, "Build", "Ship the kernel slice");
        assert!(guidance.contains("OBJECTIVE CONTRACT (canonical, harness-enforced)"));
        assert!(guidance.contains("revision 7"));
        assert!(guidance.contains("mode=Build"));
        assert!(guidance.contains("objective=Ship the kernel slice"));
        assert!(guidance.contains("Plan completion requires recorded evidence"));

        let fallback = objective_contract_read_only_default_instruction();
        assert!(fallback.contains("OBJECTIVE CONTRACT: none recorded"));
        assert!(fallback.contains("READ-ONLY analysis"));
        assert!(fallback.contains("tools that change something"));
        assert!(fallback.contains("refused until the user asks"));
    }

    #[test]
    fn gateway_prompt_instructions_own_runtime_prompt_control_order() {
        let contract = local_first_task_runtime::ObjectiveContractRecord {
            user_id: "user".to_string(),
            workspace_id: "workspace".to_string(),
            thread_id: "thread".to_string(),
            source_message_id: "message".to_string(),
            objective: "Stabilize the kernel".to_string(),
            mode: local_first_task_runtime::ObjectiveMode::Mixed,
            scope_json: serde_json::json!({}),
            allowed_actions_json: serde_json::json!(["read", "write"]),
            completion_json: serde_json::json!({}),
            status: "active".to_string(),
            revision: 12,
            created_at: 1,
            updated_at: 2,
        };

        let guidance = runtime_prompt_control_instructions(RuntimePromptControlInput {
            memory_recall_allowed: false,
            capability_router_instruction: Some("CAPABILITY ROUTER SENTINEL"),
            mode: "debug",
            objective_contract: Some(&contract),
        });

        for expected in [
            "MEMORY: you have a long-term memory",
            "OPERATIONAL PLAN",
            "MEMORY SCOPE FOR THIS OBJECTIVE",
            "LANGUAGE: ALWAYS write",
            "CAPABILITY ROUTER SENTINEL",
            "FRESHNESS / VERIFICATION",
            "EXECUTION / VERIFICATION",
            "BROWSER (delegated `browse`)",
            "DEBUG MODE",
            "revision 12",
            "mode=Mixed",
            "objective=Stabilize the kernel",
        ] {
            assert!(guidance.contains(expected), "missing {expected}");
        }
        assert!(guidance.find("MEMORY:").unwrap() < guidance.find("OPERATIONAL PLAN").unwrap());
        assert!(
            guidance.find("CAPABILITY ROUTER SENTINEL").unwrap()
                < guidance.find("FRESHNESS / VERIFICATION").unwrap()
        );
        assert!(guidance.find("DEBUG MODE").unwrap() < guidance.find("revision 12").unwrap());
    }

    #[test]
    fn gateway_prompt_instructions_prepare_chat_runtime_prompt() {
        let turn_policy = ChatTurnPolicy {
            mode: "ask".to_string(),
            read_only: false,
            autonomous: false,
        };
        let mut memory_intent = semantic_decision::MemoryIntent::safe_default();
        memory_intent.search_personal = true;
        let guidance = prepare_chat_runtime_prompt(ChatRuntimePromptInput {
            memory_intent: &memory_intent,
            capability_router_instruction: Some("ROUTE SENTINEL"),
            turn_policy: &turn_policy,
            objective_contract: None,
        });

        assert!(guidance.contains("MEMORY: you have a long-term memory"));
        assert!(!guidance.contains("MEMORY SCOPE FOR THIS OBJECTIVE"));
        assert!(guidance.contains("ROUTE SENTINEL"));
        assert!(guidance.contains("ASK MODE"));
        assert!(guidance.contains("OBJECTIVE CONTRACT: none recorded"));
    }

    #[test]
    fn gateway_prompt_instructions_keep_choice_resume_legacy_backup_out_of_sot() {
        let guidance = choice_resume_instruction_legacy_backup();
        assert!(guidance.contains("legacy backup"));
        assert!(guidance.contains("do NOT restart"));
    }
}
