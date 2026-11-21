# Spec: `live-screens`

Module `live-screens` of `CAPABILITY-MAP.md`: the terminal application as its design draws it, and live. It depends on `tui`, and starts from `4315c1da`, where `tui` ended on `main`, on the branch `feature/live-screens`.

Status: **approved on 2026-11-14**, with each of its twelve questions answered as recommended. Changed the same day by the user at the plan's approval: the design is reached whole, every component and every phrase of it, which turns question 5's answer and adds rule 5 of "What governs".

## Objective

A person opens `yunta` and what stands in front of them is decided by where the run is, and stays true with no key pressed. A run started, answered or stopped from another terminal shows on the field as it happens. A run's screen shows what the node pointed at is doing now: what happens there as it happens, the newest always in sight; a task's proof and scope; the findings as reviewers post them; a failure's output; a gate's change. A decision appears where the person is looking, under the evidence it is about, and is answered there. A new run and a setup are one screen each.

`tui` shipped the application's frame faithfully and its content not: each stage shows the page its CLI command prints, read once. The design the user approved on 2026-11-04 says otherwise: the screen is a pure function of what the log says, and no interface state can fall out of step with the run. This module makes that true at its root: what the application shows of a run is derived from a run it follows, never from a page it read.

### What governs

1. **The design**: the page the user approved (its text: `.plans/yunta-tui/proof/live-screens/design/PAGE.md`), the frames its spike recorded for 13 screens at three sizes (`design/*.txt`), and the spike's source. Its rules, which this spec carries:
   - R1. The screen is a pure function of what the log says.
   - R2. The view goes with the node at work until the person starts reading; `f` takes it up again. The screen never changes under someone who reads or types.
   - R3. What needs the person comes to where their view is: a decision appears in its place with its evidence above it; on any other screen one tinted row says it, with the key that goes there.
   - R4. The foot says only what serves here: its keys come from the acts the run offers (`RunFrame::actions`, the table the CLI's next steps come from), and an act that does not apply is not shown.
   - R5. Every closed set of the code has a screen: each `Standing`, each `PauseReason`, each `Menu`. The compiler holds it.
   - R6. Only evidence moves: a spinner's frame and the plow's cell advance with an event, never with a clock.
2. **What the user added on 2026-11-14**: the application is realtime and dynamic, every part adapted to that; what happens (tool calls, reads, edits and the rest) is listed with the most recent item always in sight at the bottom, moving up as more arrive, and the history is navigable upward by scrolling.
3. **Where the page and the recording disagree, the page governs.** The recording is the look at 120×36 and holds a spike's defects. Every difference from either is listed under "Where the design is not followed"; nothing else departs.
4. **The targets are the design, written down.** Before any code that draws a screen, its target is written by hand as text at four sizes, carrying every element of "The elements" at the place its frame draws it, with the test home's data. The user approves the targets on one page, each beside its frame. The goldens the code draws then equal them.
5. **Every component and every phrase** (the user, 2026-11-14, at the plan's approval: that design is to be reached, doing whatever it takes, its phrases included). The design is reached whole: the panel beside a new run, the list of runs and its glance, the line a person types in, each selector, each card, the palette, the help, a run's numbers and its graph, and every word they say.
   - A screen keeps a component or a sentence it has today only where its target, set beside the design's frame, shows them equal. "As today" is never said of a screen on trust.
   - An inventory lists every component and every phrase of the design, screen by screen, with the target that carries it. A screen is done when no row of it differs.
   - What a screen says today and the design does not is decided on the targets' page, one by one: said the design's way where a person acts on it, or left to the printed page.
   - Measured at the base, by the inventory: of the 66 components the design draws, 53 differ from today's or have no counterpart; of its 297 phrases, 202 do, 15 of them made up by the spike for its demo; and today's screens say 149 things the design does not.

### What happens at the base commit (`4315c1da`)

**A stage shows a page read once.** Only a run's following and the cases run hand anything back while they last (`crates/ui/src/news.rs:9-19`). The field is read on open, on return and after an act (`crates/ui/src/stage/field.rs:245-268`); the tick each second only moves the clock (`crates/ui/src/state.rs:128-131`).

**Following ends where a run stops.** `Following` asks the log every 100 ms and ends for any run that does not move (`crates/api/src/run/follow/following.rs:36`, `:269-275`): with `Stalled` for one no process drives, and with `Stopped` for a run waiting on a person, one waiting for a limit to reset, and one with a node waiting while others still work (`crates/replay/src/frame/phase.rs:25-29`). A run answered or resumed from another process is not read again. Picking a node lets the following go and `esc` does not take it up (`crates/ui/src/stage/run.rs:200-206`, `:436-452`); opening any stage over a run lets it go too (`crates/ui/src/state.rs:358-359`). The run's pointer never goes with the node at work: it stays on its node or falls to the first (`run.rs:280-286`).

**The field is read whole each time.** One reading replays the whole log of every run on the machine, reads each manifest, probes each engine and runs git twice, with no reuse (`crates/api/src/field.rs:53-92`, `:209-247`).

**Every store call opens a connection** and runs the schema batch (`crates/storage/src/async_storage.rs:46-61`). A first open on a fresh or older log takes a lock and writes (`crates/storage/src/store/opening.rs:63-95`). Nothing tells a reader that the log moved.

**The follower's fold is not replay's.** `FollowedRun::take` folds past the event where a log broke, and folds an event whatever its seq (`crates/api/src/run/follow/followed.rs:88-93`); `replay` stops at the break.

**A decision is another stage.** The run's `a` and `p` (`crates/ui/src/stage/run/acts.rs:77-78`) open `Stage::Decision` over the run (`crates/ui/src/stage.rs:174-179`), which lets the run go.

**A run's main region is the CLI's page**, with the chronicle under it scrolled from its first line, so the newest moment is the farthest from sight (`crates/ui/src/stage/run.rs:174-188`). The chronicle lists every call, one line each, and is folded again from the first event on every piece of news (`:272-287`).

**A call names its node and no session.** It is an `agent_message` with an action (`read`, `edit`, `write`, `search`, `list`, `run`, `fetch`, `call`, `submit`) and a target (`crates/core/src/events/session/payloads.rs:383-412`); a command's arguments never reach the log. The node's ledger keeps the open attempt's calls and clears them when the node ends (`crates/core/src/events/node/ledger/spend.rs:39`, `:104`).

**A task's proof and scope are in no state, and its title is not on the log.** `TaskRecord` holds no criterion and no scope (`crates/core/src/events/tasks/ledger.rs:22-72`); `task_registered` carries criteria and scope and no title (`crates/core/src/events/tasks/payloads.rs:55-64`); the ledgers ignore `scope_checked`; the receipt reads raw events for the same facts.

**The application's scroll keeps no end.** Surco can show a region's last lines (`Sight::at_the_end`, `crates/surco/src/components/scroll.rs:37-41`), but `Scroll` keeps a first line counted from the start, and `End` goes to the last line the surface last laid out, which a growing document leaves behind (`crates/ui/src/scroll.rs:24-26`, `:70-82`). Surco says "N more lines" only below a window.

**The wheel moves the pointer.** The wheel arrives as arrows (`crates/tui/src/full_screen.rs:87`), and on a run the arrows move the node pointer (`crates/ui/src/stage/run.rs:196-197`). The plan's desk does the opposite: `j` and `k` go from task to task, the arrows and the wheel move what is read (`docs/guide.md:812-814`).

**The side region is one shape**: at the left, 22 cells, kept while the main region keeps 40 (`crates/tui/src/layout.rs`).

**Starting and setting up are prompts in sequence.** `SPEC-app-core.md` had the setup end by installing a pack; it does not.

**The performer names a terminal.** A recorded answer hard-codes `Channel::Tty` (`crates/ui/src/performer/run.rs:117`).

### Who this is for

A person at a terminal, who watches and answers runs without remembering a command. And the next surface: a desktop application plugs into `yunta-ui` and gets every behavior here by handing the same requests to the same performer.

### What the module delivers

1. **One follower**, under "The follower".
2. **What the application follows is a function of its state.** `State::followed()` says it: the field wherever it stands in a repository set up, and the run whose screen is open, under every stage over it. The state asks for it when it changes; no stage starts or stops a following by hand. What arrives is folded into what the state keeps, which every stage reads. The readings a following replaces go: the field's, a run's page, a node's page, a decision, a round, a run's stats, its graph.
3. **A run's screen is a scene**, under "The run's pointer" and "The scenes".
4. **A decision is answered where it is shown**, under "A decision in place". `Stage::Decision` goes. `yunta review <run>` opens the run's screen on the plan; `yunta resolve-gate` at a terminal answers as today.
5. **The feed**, under "The feed".
6. **The field and starting by their design**, under "The elements".
7. **Every screen's frame**: the trail and the tally; the attention row; the note; the meter; the legend. Under "The elements".
8. **Surco's pieces**, each a component or a principle with its goldens and its place in the catalog: a side region at either edge, which gives way below 102 columns; the meter row; the note; the legend's far keys; a region title's far words; the selection fill of a pointed row; a boxed line to type in; a row of words with one chosen and marked; the name in block letters and the field in perspective; `Feed`. And each component Surco has that the design draws another way takes the design's shape: the card with its options, the round, a list of choices, the palette's box with its three columns, the help's box, a confirmation's box, the desk's rail, a finding's row, a checklist's row, the bars of where time went and of who spent it, the graph. Surco is one language, so a component redrawn is redrawn wherever it is drawn, the CLI's prompts and pages among them.
9. **A truthful state for what a task proves.** The ledgers learn a task's criteria and their results, its scope, the paths its sessions wrote, and every session's task, where they keep only a task's last session today. The receipt moves onto that fold and prints the same.
10. **The performer takes its channel from its surface** at construction; the binary hands `Channel::Tty`.
11. **The application says when a run comes to need the person** as the attached run does: the terminal's title, and a bell where the configuration asks for one. The screen carries the title, and one request asks the surface to call the person, once for each time a run comes to need them.
12. **The proof**: the targets approved before code; liveness tested where it lives; real runs.

### The follower

`yunta-api` follows what an interface names: the field as far as a reach goes, and at most one run read whole.

- **One reader.** One connection to the log, kept open, which refuses to write; no transaction is held between two calls, so it never holds the engine's checkpoint back. Its first open on a fresh or older log sets the log up as every open does today.
- **What a look costs.** Every 100 ms it reads `PRAGMA data_version`, and reads the log further only after another connection committed: each run's last seq, then the events past the one it holds, for the runs that moved. Once a second it reads the registry of each run whose log says it moves or asks, since an engine that dies and a question asked at a terminal commit nothing. Every ten seconds it checks that each run it holds still has its directory. A store where nothing moves costs one pragma a look.
- **One fold.** What arrives is folded by replay's own fold: nothing once the log broke, nothing for an event already folded. `replay` and `chronicle` become that fold over a slice, so the follower and `yunta status` agree on every log.
- **Facts, not frames.** It hands back states and what changed; an interface frames them at its own clock, so every time and age on screen stays true each second.
- **What it keeps and hands.** Of each run of the field: its state, its manifest, its engine. Of the run read whole it also hands every event from its first, once, and then each batch, so whoever follows folds them into its moments with the one fold; what its sessions say with how many words a slow reader missed; what it waits on a person for; and each document one of its nodes handed over, read once when the event that hands it over arrives (a task's title comes from the tasks document). What a reading remarks on, a manifest's unknown keys, is said once for each run read, never at each look.
- **What a run asks**, a decision with what it shows or a round's questions, is read once per pause, from the state the follower already holds, off the look's path. A document that fails today's shape arrives as unreadable in place. A read that finds no decision where the log said one waits means it is answered.
- **Errors stay with their run.** A run that cannot be read is said as unreadable and the rest go on; a run that leaves the log leaves the field.
- **It never ends by itself.** `yunta status --follow` follows through it and stops where the run does not move and awaits no successor, printing what it prints today; it reads the run's first lines from the log.
- **It only reads.** The test that holds `status --follow` to that holds the application's follower.

### The run's pointer

- **Who leads.** The run leads its screen while the node pointer is the run's, the finding pointer is the run's, and the pane's feed stands at its end. A person takes the node pointer by moving along the nodes or by putting a prompt aside; the finding pointer by moving along the findings; the pane by scrolling its feed back. Scrolling what stands above a card takes nothing.
- **While the run leads**, its node pointer stands on the node the run waits on a person at, when one waits; else on the first node at work in the workflow's declared order, kept there while it works; else on the last node that ran. A pause or an end that names no node (the token cap, no room, a stop, a stall, a finished or failed run) shows the run's own scene. Its finding pointer stands on the first finding with no answer, else on the last.
- **While the run does not lead**, it moves neither a pointer nor the pane, a prompt that comes is said by the attention row, and the scene is the pointed node's own.
- **Giving it back.** Scrolling to the feed's last row gives the pane back. `f` gives back all three at once: both pointers to the run and the feed to its end.
- **How long.** What the person took is kept while the run's screen is open, under any stage over it; leaving to the field gives it back to the run.

### The scenes

The scene is a pure function of the run kept, of what it waits on a person for, of the node pointed at and of whether the run leads. A prompt (a card, a round, the desk) stands only while the run leads. The function that makes it uses `match` alone and no wildcard arm over `RunPhase`, `PauseReason`, `Menu`, the node's `Standing` and its kind. A card is the decision's prompt at the pane's foot, built from the escalation the engine rebuilds, drawn and worded as the design draws its cards. A run parked on a departure owed or on a loop's cap is rebuilt by the engine as a failure's menu with that cause (`Menu::Departure` and `Menu::LoopCap` are what an engine asks at its own terminal), so what a card rests on is read from the cause as well as from the menu, and the card stands on the same grounds either way.

| What the log says | Scene | What the pane shows, top to bottom | What it asks |
|---|---|---|---|
| the node runs a session | work | what it hands over, each artifact with its standing; what it says; its feed | nothing |
| the node runs a command or a check | work | what it runs; its result and the tail of the output kept, once it has them; its feed | nothing |
| the node runs a loop | board | each task at work: its title and attempt, how it is proved (red or green, by which command), what the run holds every task to, its scope with the paths written and those granted; then the feed | nothing |
| findings posted, and the node reviews or fixes | findings | how many are answered and which nodes posted; one finding a row, a pointer; the pointed finding (where, what, its answer once it has one); the node's feed | nothing |
| `Questions`, `AnswersRefused` | round | the request above; each question in turn, a refused answer under its question; nothing recorded until the last | answer |
| a gate that shows a plan, the change it counted holding no file | desk | the plan's desk as the design draws it (`plan`), the gate's options at its foot | a number |
| a gate with no plan, or whose counted change holds a file; `Menu::Console` | card | what the gate shows, as the gate's page draws it above a card today; the change by file; a plan it shows one `r` away | choose |
| `Menu::Failure`, `NodeFailed`, an escalation of a failed node | failure | the failing output's tail; kind, attempt, how long it took; the card | choose |
| `Menu::Scope` | card | the write the fence refused, when one was; the path and why | grant, deny |
| `Menu::Departure`, or a failure whose cause is a departure owed | card | what the plan decided, then what the session says instead; the options the engine offers | choose |
| `Menu::Budget`, `BudgetExhausted`, `Menu::LoopCap`, `LoopOverrun`, or a failure whose cause is a loop capped | card | what is spent against its cap | choose |
| an answer recorded, its resume not yet on the log | work, still | the answer as recorded; that it goes on when an engine takes it | `r` when no engine does |
| a pause on the log, what it asks not yet read | work, still: the board where its node is a loop | as its node at work draws, with no card yet | nothing |
| a pause whose escalation carries no menu | standstill | the pause in the log's words; that no option answers it here | `r`, `x` |
| `AwaitingReset` | work, still | as work, with when it goes on alone; no tint, no spinner | nothing |
| `ExternalGate`, `Menu::Published`, `PullRequestClosed` | pull request | the link and how it stands, read from the forge when the scene opens | `o` opens the pull request; the run's page is in the palette here |
| `ChildPaused` | the child | the child becomes the run read whole, shown in its place; the trail leads up to its parent, and, with no prompt standing, `esc` goes up to the parent; a prompt of the child's is put aside first, as any prompt is | the child's |
| `NoRoom` | standstill | what is free against what is needed | `u` opens upkeep over the run, which collects once confirmed; then `r` |
| `UncertainOrphans`, `Blocked` | standstill | what is left half done and what to check before going on | `r` |
| `Cancelled`, `CancelledAfterCrash`, `GateAborted`, `Recorded` | standstill | where it stopped and why, in the log's words | `r`, `x` |
| stalled | standstill | where it stopped: no process drives it | `r`, `x` |
| a question open at another terminal | asked elsewhere | the node it asks about, since when, and the process that asks | nothing here |
| created, no node started | work | what runs first | nothing |
| finished, reported | receipt | criteria, baseline, scope, review, event chain (verified once, when the scene first shows; nothing is written); cost, change, pull request; under a report, the findings that block | nothing |
| failed, promoted, broken | ended | the failure's evidence; the successor, which takes the screen while the run leads; where the log stops making sense | nothing |
| a node pointed at while the run does not lead | its own; ended, for the node a failed, promoted or broken run's end names | by its standing: at work, as its work, board or findings scene; needing the person, what it waits on and its feed, the prompt one `a` or `f` away; ended, what it did (its findings, as the findings scene, for a node that posted or answered them), its outcome's summary and its feed whole; never ran, what it runs; skipped, why the mode leaves it out; the node a failed, promoted or broken run's end names shows the end, whoever points at it | nothing |

Every pane but the findings', the receipt's and the round's opens with its title: the node and its kind, and at the far end who works it, its task, its check or its assignee.

### The feed

- **What it lists.** What happens at a node, from the log, for every attempt: each call of its sessions, and what the engine does there between them (a session opened, a write refused, a file formatted, a check and its result, an artifact handed over, a finding posted, a scope asked and answered). A row names each attempt where it opens.
- **A row.** A call is its verb in a column as wide as the widest verb, then its object as `target::said` names it, the newest row strong. An engine's moment is `chronicle::line`'s words without its subject. A row is cut at its edge, never wrapped, and ends in an ellipsis. Each row's words are those `status --follow` prints for the same moment, after its time and its subject.
- **Its heading** says how many calls the open attempt made.
- **Where it stands.** The work, board and findings scenes and a node's own scene end with the node's feed. It fills downward from under its heading; once it holds more rows than the pane leaves it, its newest row is the pane's last row and the older ones stand above its window. The blocks above it stand still and give way before it does, and the feed's window takes the rows they leave. A card's, a round's, the desk's, a standstill's, the receipt's and the ended scenes show no feed, as the frames draw them: a node's feed is on its own scene, one `esc` from its prompt.
- **Scrolled back.** In a pane that ends with a feed, only the feed scrolls. It stays on the rows the person reads: its place is the feed row at its window's top, counted from the feed's start, since rows only append, so what grows above or under it never moves what is in sight. It says how many rows stand above its window and how many under it, a number that grows as more arrive. `End`, `f`, or scrolling to its last row goes with the end again.
- **With several tasks.** The board ends with one feed, the node's, under every task. Where each call names its session (question 2) and several tasks work at once, each row names its task after its verb, and each task's scope marks the paths its own sessions wrote.
- **What it costs.** Moments arrive from the fold; a draw lays out only the rows in sight.
- **The run's whole chronicle** stays reachable from the palette, as a feed of its own.

### A decision in place

- **Where.** The card stands at the pane's foot, under its evidence. A round takes the nodes list and the pane. A plan's desk takes everything between the rule and the attention row.
- **When.** It appears by itself when the run comes to need the person and the run leads. While the run does not lead, under a stage over the run, or on another screen, the attention row says it, and no key changes meaning until the person goes there with `a` or `f`.
- **Its keys.** The arrows and the wheel move only what is read above a card or in the desk's pane, and never change a card's choice. On a card `j k`, `tab` and the numbers choose; on the desk the numbers choose, and `j k` and `tab` go from task to task. A prompt's other keys (a card's note, the plan read from a card, a round's way back) are the same at the CLI's prompt and here, so a person answers alike at both.
- **An answer recorded** is said in a note, in the design's sentence for that option.
- **Words.** An option that asks for words opens its typed line in place: at the card's foot, or at the desk's foot in place of its options. `enter` records it; `esc` returns to the options.
- **`esc` puts it aside.** The node pointer becomes the person's on that node, whose own scene shows what it waits on and its feed, so they can read it and the other nodes before deciding; `a` or `f` brings the prompt back as they left it. `esc` again, or `q`, leaves the run as it does with no prompt, with a note saying the run waits.
- **A prompt belongs to its pause.** News of the same pause never rebuild it, so what the person typed or picked stays. A decision answered from elsewhere takes the prompt away, even under a person typing on it, since what they answer no longer waits: a note says it is answered and by whom, in the log's words, and what was typed goes with the pause.

### What moves and what holds

- **Moves with no key:** every time and age, each second, at the interface's clock; marks, a spinner's frame and the plow's cell, with an event (R6); the ribbon, the tally, the attention row, the meter, counts; a feed that is at its end; the field's rows, each moving to its new group as its run moves.
- **Holds until a key:** a pointer, kept by the run, node or finding it stands on, never by a position; the node a pane shows while the run does not lead; every pick, every scroll place, every typed character; an open palette, help or confirmation, with its pointed row and its target.
- **A screen typed into does not change:** a run started elsewhere while the first request is being typed shows in the tally, and the screen stays until the person leaves it.

### Keys

Where nothing is typed, `j` and `k` move a pointer, and the arrows, the wheel, `PgUp`, `PgDn`, `Home` and `End` move what is read; the field, which reads nothing beside its list, moves its pointer with both. Where a line is typed (the first request, a new run, a round, the palette, a typed line on a card or the desk, a setup row being changed), every printable key types, and only `ctrl-c`, `tab`, `shift-tab`, `enter`, `esc` and the arrows act: `↑↓` move the pointer of the list in focus (the workflows, a round's choices, the palette's rows), and `←→` move in the line or change the chosen word of a row of words. A wheel turn there moves that pointer, since it arrives as arrows: a round records nothing until its last answer, and `shift-tab` goes back to change one.

| Where | Keys |
|---|---|
| everywhere, not typing | `ctrl-c` leaves; `/` the palette; `?` the keys; `n` a new run, where no prompt stands; `a` goes to what the attention row names: on a run whose own decision it names, that prompt as it was left; else that run's screen, in place of this one |
| the field | `j k` and the arrows move; `enter` opens; `space` marks; `x` closes the marked, or the pointed, once confirmed; `r` resumes where it applies; `d` lists the closed too; `e` reaches the whole machine; `q` leaves; `esc` does nothing |
| a run | `j k` along the nodes; the arrows and the wheel through the pane; `space` a page on; `f` gives the lead back to the run; `s` what it cost; `g` its graph; `r` and `x` where the scene offers them; `enter` nothing beyond a prompt; `esc` and `q` go to the field, with a note that says truly whether the run keeps going, waits or ended |
| findings | `j k` along the findings; `tab` hands `j k` to the nodes and back, and the legend says which they move; the arrows through the pane |
| the workflows | the keys it has at the base; `enter` on a workflow opens the new run's screen with it chosen |
| a card | `j k` and `tab` choose; a number picks; `enter` answers; `n` adds a note; `r` reads the plan it offers; `esc` puts it aside |
| a round | a line typed, or a choice picked with the arrows and narrowed by typing; `enter` answers; `shift-tab` the question before; `esc` puts it aside, nothing recorded |
| the desk | `j k` and `tab` from task to task; the arrows through what is read; `p` proof; `d` what is decided; a number, then `enter`; `esc` puts it aside |
| the first request | type; `enter` goes on to the new run's screen with what is typed; `esc` to the field where there is one, else leaves |
| a new run | `tab` and `shift-tab` from row to row; on a typed row, type, `←→` in the line; on the workflows, `↑↓`; on the mode and the adapter, `←→`; `enter` starts, or says what is missing; `esc` goes back and keeps what was typed |
| a setup | the pointer starts on the row that writes; `j k` along its rows; `enter` on a row changes it (a choice among the adapters this machine answers for; a typed line for a name, a branch, a model), on the last row writes; `q` leaves, nothing written |

Keys whose meaning changes from the base (question 9):
- `a`: on a run it answered, on the field it reached the whole machine. It goes to what needs the person, everywhere; the reach is `e`.
- `n` on the field opened the run waiting longest, which `a` now does. It starts a new run. On a card it still adds a note.
- `q` away from the field left the application. It goes to the field. `esc` on the field left too. It does nothing there.
- `f` on a run toggled the following. It gives the lead back to the run.
- `tab` on the findings did nothing. It hands `j k` to the nodes and back.
- `enter` on a node picked it. It does nothing, since the pointer already shows the node.
- A run's `p` (read the plan) and `w` (its status page) go: the desk is the gate's own scene, and the run's screen is its status.

Keys the design does not name keep what they do and are listed in help and the palette, not the legend: `c` cancels; the field's `w` the workflows and `m` the machine; a run's `e` its receipt, `v` verifies, `o` its page; the machine's `u` upkeep.

### Sizes

- The targets are written at 120×36 and 80×24 in Unicode, 120×36 in ASCII, and 60×14, the least terminal. Wider than 120 the side regions keep their width and the main region takes the rest; taller, the pane and the lists take the rows.
- The side regions (a run's nodes at the left, 31 cells and a gutter of 3; the field's glance at the right; a new run's facts; a setup's second candidates) stand from 102 columns, the design's own threshold: a screen 100 cells wide inside its margins. Below that the main region takes the width and the trail still names the node pointed at.
- The legend's far keys and the palette's command column stand from 100 columns; below, the legend ends with the key that lists every key, as today.
- The ribbon names each stretch where every name fits, and draws strokes alone where they do not (question 3 says by which name).
- Three things never give way: the row that says what `enter` does, a card's options, and a feed's newest row. Evidence above a card gives way from its top and never overlaps it. A scope's paths give way before a feed. A cut ends in an ellipsis and never splits a number. A list that hides rows says how many, above and below. What a pane cuts above its feed is read whole on a taller terminal and on the node's printed page. The targets fix the rest of each screen's order, and the user approves them.

### The elements

What each target carries, with the frame that draws it. "Real" is what stands there in place of the spike's constant.

| Id | Element | Frame | Real |
|---|---|---|---|
| C1 | trail: `yunta`, then the place (the repository's path on the field; the run and the node pointed at; `new run`; `set up`) | every seed, row 1 | |
| C2 | tally at the row's far end: how many need the person, how many stalled; `nothing needs you` | row 1 | the followed field |
| C3 | rule: an empty furrow across every cell | row 2 | |
| C4 | legend: the keys that serve here; the far pair `/ anything · ? keys`, left out where a line is typed and under 100 columns | last row | rule R4 |
| C5 | attention row, tinted: on a run's screen whose own decision is not in sight (the run does not lead, or a stage stands over it), that decision; elsewhere, the run that waited longest on the person among those answered here; what it asks, `a go there`. A run asked at its own terminal is counted in the tally and listed under `asking now`, never named in this row | `field` row 35 | on every screen, but where the decision itself stands |
| C6 | meter: tokens, one bar a node, time at work, time waiting on a person when it is not zero | `work` row 34 | the frame's tokens and times |
| C7 | palette (a row: what it does, its key, the command that does the same; typing narrows it) and help | `work+palette`, `work+help` | the box, its columns and its words as the frames draw them; its rows are every act the code offers, the ones the design does not name among them |
| W1 | every phrase: headings, labels, legends, a card's question and each option's name with what it does, the sentence that says an answer is recorded, a group's name, a checklist's rows, a confirmation's sentences | every frame | the design's words, owned by render; what the log holds where the spike made words up |
| E1 | the note: a done mark and what an act did, in the attention row's place until the next key; news leave it | `work+q` row 35 | |
| E2 | the selection fill on a pointed row, to its region's edge | `field`, the rail | |
| H1 | headline: mark, workflow and mode, standing, `for 7m34s`, the handle at the far end | `work` row 3 | |
| H2 | ribbon: a name over each stretch; the plow on the stretch at work | rows 4-5 | names by question 3 |
| H3 | gist: one line saying what holds the run | row 6 | `state::gist` |
| N1 | a node a row: pointer, mark or spinner, name, how long it took | `work` rows 8-22 | |
| N2 | a loop's tasks under it, each with its mark | `build` row 14 | |
| N3 | one spinner frame for every node at work | `review` rows 15-17 | |
| E3 | `f follow what it is doing` under the nodes while the run does not lead | `work+j` row 24 | |
| E16 | the mark of a node sent back | state 189 of `out/frames.json` (`plan`, `2`, `enter`) | |
| P0 | the pane's title and its far words | `work` row 8 | the session's runner; the task; the check; the assignee |
| P1 | work: `what it hands over` (`◌ tasks  not yet`, or accepted); `what it says`; `what it is doing · N calls` and its rows; `session opened` before the first | `work`, `work+event` | the feed last (see "The feed"); N is the open attempt's calls |
| P2 | board: the task with its spinner, title and attempt; `proved` red or green, `by`, `keeps`; `its scope` with a mark a path, `granted by you`; its feed | `build`, `build+event` | title from the tasks document; criteria from the ledger; a path marked when a session of the task writes it |
| P3 | round: `you asked`, the card over the nodes list and the pane, an answered question folded to one row | `ask`, `ask+enter` | |
| P4 | desk, with its confirmation row | `plan`, `plan+1` | the plan's own content; the rail's groups, their names and the pane's far words as the frame draws them |
| P5 | scope card: the refused write, the path, why, `grant` and `deny` with what each does | `scope` | the task's own reason; the design's words for the rest |
| P6 | failure: the tail, `kind`, `attempt`, `took`, the card | `failure` | |
| P7 | findings: the count, the list with severity, the pointed one, `fixed:` with its answer | `review` and three events on | posting nodes for reviewers; the node's feed last |
| P8 | ship: the change by file with bars; findings and criteria in a line; the card | `ship` | the four largest files and how many more |
| P9 | receipt: the checklist, `cost`, `changed`, `pull request`, the harvested field under it | `receipt` | |
| E4 | a node's own page: `It has not run yet.`, or `stands`, `took`, `tokens` and its summary | `work+j`, `work+k` | plus its feed |
| F1 | the field's first row: `› what do you want to build?  · n` | `field` row 3 | |
| F2 | groups: `asking now`, `needs you, oldest first`, `stalled`; and `stopped`, `running`, `closed` where there are any | `field` | render's groups; a run that cannot be read under `unreadable`, with why |
| F3 | a run a row: pointer, mark, handle, mode, furrow, what holds it, how long ago | `field` rows 6-23 | |
| F4 | the glance at the right: headline, ribbon without names, gist, `progress`, `tokens`, `time`, `slowest`, `branch`; `what you can do`, a key a row | `field` rows 3-20 | acts from `RunFrame::actions` |
| E5 | the confirmation to close: what closing does and the command that does the same | state 114 of `out/frames.json` (`field`, `j`, `x`) | names every run it closes |
| S1 | first request: the name in block letters; `What do you want to build?`; the boxed line; the workflows, each with what it does; the field in perspective | `empty` | |
| S2 | new run: the boxed request and its hint; `workflow`; `mode`, the chosen one marked; `what runs`, with where it stops for a person; at the right `what it usually takes` and what the machine answers for | `compose` | the workflow's other inputs and the adapter as rows; usual cost from three past runs on; an admission refused is said in place |
| S3 | setup: the name in block letters; one sentence; `what this repository is`; `who does the work`, a role a row with its candidates; the row that says what `enter` writes; the field in perspective under it, as S1 | `setup` | the committer; rows changed in place |
| S4 | the pack's audit, then a confirmation as E5's | no frame | what `yunta pack add` shows before it installs |
| M1 | a screen redraws when an event arrives | `work+event` | and each second for times |

A run's stats and its graph are drawn as the design draws them (`work+s`, `work+g`): the sentence under the ribbon, each labelled row, where its time went with its bar and what each mark means, who spent it with a bar a role; the graph's headline and its nodes. They read the run kept, so they move with it. The design draws no frame for the machine, upkeep, the workflows with their checks, cases, new workflow and packs, a document's shape, a workflow's history or a run's evidence: each keeps the facts it shows today and is designed in its target, in the design's frame and with the design's components. The machine and upkeep read the machine, not the log: they are read when opened, as today (question 12). Outside a repository the application opens on its signpost, as today, and follows nothing until the person reaches the machine's field.

### Names and form

- `yunta-storage`: `LogReader`, the log read through one connection kept open, which refuses to write and says whether another connection committed. Not `AsyncStorage`, which opens a connection for each call.
- `yunta-replay`: `RunState::fold` replaces `apply` for every caller outside replay: one event folded as `replay` folds it, with its moment.
- `yunta-api`: module `follow`, moved up from `run/follow`: `Followed { field, run }`, what is followed; `Following`, made by `Project::follow`, with `refollow` and a `next` that never ends by itself; `FollowNews` and `RunNews`. `Awaited`, a decision or a round, stands in `answers.rs` beside the two it sums; not `WaitingOn`, where a run is parked.
- `yunta-ui`: `State::followed()`, `Request::Follow`, `Input::FollowNews`; `Request::CallThePerson`, asked once each time a run comes to need them. `FollowedRuns { field, in_view }` and `RunInView`, what the state keeps of what it follows; not `yunta-api`'s `FollowedRun`, one run as the follower hands it, which stays. `Outcome::Followed` goes, since a following no longer ends. `Scene`, what a run's main area stands as, with `Grounds` for what a card's decision rests on, drawn above it (not `Evidence`, a criterion's proof; not render's `Gate::AboveTheCard`, the page a gate prints over a card), and `Standstill` for why a run stands with nothing to answer: a pause, or no process driving it. Its arm is `Scene::Standstill`, since `Standing::Stopped` means a person stopped the run. `PointedBy`, the run or the person, kept for the node pointer and for the finding pointer; that the run leads is derived from both and from the pane's place, never stored. `Scroll` gains `ScrolledTo`, the end or a row, which maps onto Surco's `Sight::at_the_end` and `Sight::from_line`. The run's `follows` flag goes with the following it asked for.
- `yunta-surco`: `Feed`, the rows in sight with how many stand above and under. A region holding a feed is laid out as its feed, as a region holding a desk is laid out as its pane. The CLI's `surface::feed::Feed`, a queue of beats, is renamed for what it is.
- `yunta-render`: a pane's parts are values in `blocks`, as `FailureDetail` is; a feed's rows are `chronicle`'s; the meter is `watched::spent`, where `watched::footer` is named by its place.
- The mock adapter: `MockStep::AwaitsWord`, which names a word and goes on once `yunta_testkit`'s `Hold` gave it. `Hold` gives one word today, which stays; it learns to give each word a test names, so a test lets a session's calls go one at a time.

### Decisions the plan starts from

1. **Targets first**, approved by the user (rule 4 of "What governs"). The scenes the design never drew are designed there.
2. **The follower before the scenes.** The application's present screens become live on it first; the scenes then replace them.
3. **The ledgers learn before the panes read.**
4. **Words are the design's, and render owns them.** A thing said on two interfaces is said in one sentence, so what the CLI prints of the same thing changes with it: its goldens are blessed, and their diff is reviewed at the checkpoint of the phase that changes them (question 5).
5. **A setup installs a pack, as `app-core`'s spec had it and the design draws it**: `yunta/fragua`, through the pack's own install, its audit shown and confirmed first; declined or unreachable, the setup is written without it.

### Assumptions

1. One module, in phases (question 7).
2. The feed lists the engine's moments beside the calls (question 1).
3. Times and ages tick each second, as the application's clock does at the base: R6 is read as about marks (question 11).
4. A new run's screen also asks the workflow's other inputs and the adapter, which the design does not draw: a run cannot start without the first, and `yunta run --adapter` exists.
5. A setup's rows can be changed, as the user chose on 2026-11-10.
6. "Only evidence moves", runs closed several at a time, a run asking at another terminal shown read only, and the mouse not captured stay as the map assumes.
7. `.agents/architecture.md` stands 9 bytes under its cap: the rules this module adds fit by rewording what is there; raising the cap is asked first.

### Where the design is not followed

Against the page:
- An answered finding keeps its row with a done mark and its answer under it, where the page strikes it: a strike has no plain form, and every layer says the same.
- The engine's options decide a departure's card, where the page lists two.
- No room is a standstill whose act is collecting, where the page draws a disk card: the engine rebuilds no decision for it.
- `n` starts a run wherever no prompt stands, where the page says from any screen: on a card it adds a note, as the CLI's prompt does.
- Times and ages tick each second, where the page moves nothing by a clock: a time that stands for a minute while a node works reads as a screen that stopped (question 11). Marks move with events alone.

Against the recording:
- The attention row stands on every screen, as the page says.
- A run asking at its own terminal is never named in the attention row, where the recording names one: `a` cannot answer it here.
- The findings' pane and a node's own scene end with the node's feed, which no frame draws.
- On the findings `tab` hands `j k` to the nodes, where the recording gives no way to another node.
- A new run moves between rows with `tab` and `shift-tab` and changes the mode with `←→`, where the recording's legend says `tab workflow · h l mode`: letters are typed there.
- A new run also holds a row for each other input its workflow declares and a row for the adapter, which no frame draws, each in the design's own components: a run cannot start without the first, and every interface offers what `yunta run --adapter` offers.
- `j k` and the arrows are apart, where the spike reads them as one key (question 4).
- The feed closes the pane, where the frames draw it above what the node hands over and says: the newest row has to be the last.
- `esc` puts a prompt aside and stays on the run, where the recording goes to the field: a person reads the other nodes before deciding.
- A run asking at its own terminal is read only, as the page says.
- `q` on a run goes to the field, as its legend says; the recording's help says it leaves.
- A round goes back with `shift-tab`, the prompt's own key, where the legend says `h`: a letter is typed there.
- A setup's rows are changed in place and one `enter` writes, where the recording walks three steps.
- The mode chosen on a new run is marked, not only filled.
- What the spike made up is replaced by what the log holds (the column "Real").
- Defects are not targets: the lost act row and the cut sentence of the setup at 80×24; the attention row cut inside a number; the failure's card over its fields; the board's lost feed; a count of calls that is not its rows; a pointer kept by position; an overlay that discards a half answered round; the first request's legend naming keys that type; a note that says a waiting run keeps going.

### Out of scope

- A desktop application: `desktop` stays reserved in the map.
- Taking a question from the terminal that asked it.
- A repository set up from another process while its setup screen is open.
- Where the CLI's inline prompts stand in a stream, and its live region: both stay. A prompt is one model on every interface, so a key a prompt gains here it gains at the CLI (a card's numbers, words typed at the desk's foot), and its drawing is Surco's, so a component the design redraws is redrawn there too.
- Capturing the mouse.

## Tech Stack

Rust 1.98.0. No new crate. `yunta-storage` gains a kept reader on `rusqlite` 0.41.1; `yunta-tui` stays the only crate that names Ratatui and crossterm. By question 2, `agent_message` gains one optional field: `cargo xtask schema` and `crates/core/schemas/` change, and the engine's writer of calls.

## Commands

```sh
# Before editing a file, and before finishing with no paths
cargo xtask docs --about <files>

# Test
cargo test -p <package> --test <target>

# Lint and checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask smells --check
cargo xtask docs --check

# The suite in groups, and the gate: every step of the ci job, once, CARGO_INCREMENTAL=0
python3 .plans/yunta-tui/tools/suite.py <group>
sh .plans/yunta-tui/tools/gate.sh /Users/eulke/development/yunta-tui <log dir>

# Output meant to change, and a changed document type
YUNTA_BLESS=1 cargo test -p yunta-surco -p yunta-render -p yunta -p yunta-engine
cargo xtask schema
```

`cargo` means `sh .plans/yunta-tui/tools/cargo-in.sh /Users/eulke/development/yunta-tui <args>`.

## Project Structure

```
crates/storage/src/log_reader.rs        the kept reader
crates/replay/src/lib.rs, frame/        the one fold: state and moment in a pass
crates/core/src/events/tasks/, scope/, node/   the ledgers' new facts
crates/core/src/events/session/, crates/core/schemas/, crates/engine/src/task_cycle/   a call's session (question 2)
crates/api/src/follow/                  Followed, Following, FollowNews
crates/api/src/answers.rs               Awaited
crates/ui/src/followed_runs.rs          what the state keeps of what it follows
crates/ui/src/stage/run/                the run's stage: scene.rs, the pointer, prompts in place, each scene's pane
crates/ui/src/stage/{field,start,setup,first_run}.rs
crates/ui/src/scroll.rs                 the end, or a row
crates/surco/src/components/            Feed and the screen's new pieces, each with its terminal drawing
crates/render/src/blocks/               a pane's parts
crates/render/src/chronicle/            a feed's rows
crates/tui/src/layout.rs, drawn.rs      the sizes
crates/adapters/src/mock/               the step that waits for a word
crates/testkit-ui/src/                  recordings of what is followed; the walk fed by them
crates/tui/goldens/screens/             the targets
```

## Code Style

The repository's: types that admit only valid states, an owner for each rule, no alias or re-export, a name that says what a thing is, a `///` reason on every numeric const, English in the present tense. The scene shows the form: a closed sum with one arm for each layout, made by a function that matches each closed set with no wildcard, so a new `PauseReason` fails the build until it has a screen.

```rust
/// What a run's main area stands as, for the node pointed at. Not a `Stage`: where a
/// person is in the application. Not a `Moment`: one thing the log says happened.
pub(crate) enum Scene<'r> {
    Work(&'r NodeFrame),
    Board(&'r NodeFrame),
    Findings(&'r NodeFrame),
    Round(&'r WaitingQuestions),
    Desk(&'r WaitingDecision),
    Card(&'r WaitingDecision, Grounds<'r>),
    PullRequest(&'r PauseReason),
    Standstill(Standstill<'r>),
    AskedElsewhere(&'r Asking),
    Receipt(&'r RunFrame),
    Ended(&'r RunFrame),
    Node(&'r NodeFrame),
}
```

A failure is a card over the failing output, and a run that wakes alone is work that stands still: a layout has one arm, and what differs inside it is data.

## Testing Strategy

- **The inventory**: every component and every phrase of the design, screen by screen, each with the frame that draws it, what stands for it today, and the target that carries it. A checker holds that each row's words stand in its target, and that no target says a phrase the inventory gives another form.
- **Targets**: text written by hand, approved by the user, then one golden for each scene and screen at the four sizes, drawn on Ratatui's test backend from the test home, equal to its target. The targets are text: tints, fills and strength are held by each new Surco piece's goldens on its 16-color layer.
- **What a golden cannot hold**, each with a test in `yunta-ui`: a run that wakes alone shows no spinner; the forge is read once when the pull request's scene opens; `esc` on a child goes up to its parent; a successor takes the screen only while the run leads; no key answers a run asked elsewhere; a repository with no run opens on the first request.
- **The fold**: a property in `replay/properties`: folding a log in any chunks equals replaying it, state and chronicle; a broken log folds as it replays.
- **The follower**, on real logs in `yunta-api`: a store where nothing moves costs no query past the pragma; a run's row and its whole reading agree at every look; an engine killed shows as a stall within two seconds with nothing committed; a run that cannot be read is said and the rest go on; a collected run passes through its missing directory and leaves; a run parked then resumed from another process is read on; what a run asks is read once per pause; a document that fails its shape arrives unreadable.
- **The reader only reads**: the test that holds `status --follow` to it holds the application's follower.
- **Liveness at a terminal**: the binary driven at a pseudo-terminal while another process moves the run, the screen asserted to change with no key. A session is held between two calls by `MockStep::AwaitsWord`, never by time, and each assertion is an order, not a duration: the row stands before the next call is let go.
- **What holds**: with a feed scrolled back by the arrows, more calls let go one at a time leave the rows in sight where they are and raise the count under them; a card appearing while the person points another node changes no key's meaning; a half typed answer survives news of the same pause; an open palette keeps its pointed row.
- **The walk**: every state reachable, fed what is followed as it is fed outcomes; bounded, since the fold takes an event once; it holds that `State::followed()` names the run under every stage opened over it.
- **Same words**: each feed row's words are those `status --follow` prints for the same moment, after its time and subject.
- **The scenes are exhaustive**: the compiler, a lint that denies a wildcard arm on the module, and a test that makes a scene for every `Standing`.
- **Only evidence moves**: with no event, two drawings a second apart differ only in times.
- **A frame's cost**: the probe draws a run with a feed of ten thousand rows at 200×60, the median of two thousand drawings.
- **Each fix red first**; a move or a rename is guarded by the tests it touches passing unchanged.
- **Real runs**: fragua in a sandbox, three runs before and three after, with the application open on the field and without; the time between nodes from `yunta stats`. The application's processor time, open for a minute on the machine's field with nothing moving.

## Boundaries

- **Always:**
  - Write a target and have the user approve it before the code that draws it.
  - Give every sentence to render and every mark and tone to Surco.
  - Keep a reader reading only.
  - Stage paths by name; run the ci job's steps with `CARGO_INCREMENTAL=0` before each phase's commit, then `cargo clean`.
  - Hold every rule `cargo xtask docs --about` prints.
- **Ask first:**
  - A new crate, or an event kind or payload field beyond the one question 2 adds.
  - A change to what a CLI command prints, beyond what questions 2, 5 and 6 name: a session on each call, a phrase or a component the design redraws, and a node's page.
  - A cap raised in a check.
  - A scene, a key or an element this spec does not name.
- **Never:**
  - A page read once where the follower can say it.
  - A following started or let go by a stage's own hand.
  - A frame handed across the API in place of the facts it is made from.
  - A clock read below the surface, but by the follower to pace its looks.
  - Ratatui or crossterm outside `yunta-tui`; the mouse captured; a run hosted by the application.
  - A wildcard arm, an `if let` or a `matches!` deciding a scene.
  - `git add -A` or `git stash`; a document cited in code, tests, commits or the pull request.

## Success Criteria

1. With the application open on the field and no key pressed, a run started, answered, cancelled or finished from another process moves its row to its group and changes the tally and the attention row; a killed engine shows as a stall with nothing committed. Each is asserted at a pseudo-terminal.
2. A run parked on a person and answered from a shell goes on in the application with no key pressed: the card goes, a note says who answered, the scene becomes work.
3. A held session's calls appear one by one as the last row of its node's feed, the pane's last row. Scrolled back with the arrows, the rows in sight stay as more arrive and the count under them rises; the pane stays on that node when the run moves on; `End` returns to the newest. A finished node's feed lists every call of every attempt.
4. Picking a node, opening stats or the graph over a run, and coming back never stop the run's screen from moving; the walk holds it.
5. A variant added to `Standing`, `PauseReason` or `Menu` fails the build until it has a scene.
6. Every scene and screen equals its approved target at the four sizes, and no row of the inventory differs from the design: every component it draws and every phrase it says is on its screen; nothing is cut inside a number; no act row, card option or newest feed row is lost at 80×24 or 60×14.
7. While the run leads, its pointer goes with the node at work and a prompt appears by itself. Once the person moves along the nodes or the findings, scrolls a feed back or puts a prompt aside, the run moves neither a pointer nor the pane, and a prompt that comes is only said by the attention row, where `a` brings it. Scrolling to the feed's last row gives the pane back, and `f` gives back everything.
8. A decision is answered in place, under its evidence, and its answer is on the run's log; an option that asks for words takes them on the card and on the desk; `esc` puts the prompt aside and what was typed is there when it comes back; `yunta review <run>` opens the run's screen on the plan.
9. A repository set up with no run opens on the first request. A new run starts from one screen and leaves what `yunta run` leaves with the same choices. A setup is written from one screen and leaves what `yunta init -i` leaves with the same answers, with `yunta/fragua` installed as `yunta pack add` installs it when it is confirmed.
10. The pointer on the field stays on its run while rows move; a confirmation closes the runs it named; an open palette keeps its pointed row through news; a note stays through news until the next key; the first request's screen stays under a typist while a run starts elsewhere; the terminal's title says a run needs the person when it comes to, and the bell rings only where the configuration asks.
11. With no event, two drawings a second apart differ only in their times, and every time and age on screen is true to the second. The run's whole chronicle opens from the palette, its newest moment in sight.
12. The follower and `yunta status` agree on every log the suite holds, a broken one included; at the first phase's end `status --follow` and the receipt print what they print at the base, the receipt read from the ledgers; from then on they differ from the base only by the design's phrases and components, each difference a row of the inventory.
13. A store where nothing moves costs the follower one pragma a look. A drawing with a feed of ten thousand rows takes under 2 ms at 200×60. Open for a minute on a field where nothing moves, the application spends under 2% of one core.
14. Real fragua runs with the application open keep their time between nodes inside the spread of three runs without it; the release binary stays under 40 MiB.
15. The suite, every ci step and CI on Linux and macOS are green. `README.md` and `docs/guide.md` say the keys and the screens as they are; `.agents/architecture.md` carries the follower's rule with its test; the map's row is true.

## Open Questions

None. All twelve were answered when the spec was approved, each as recommended:

1. **What the feed lists:** each call, and what the engine does at the node between them (a write refused, a check's result, an artifact handed over). The calls alone, as the design draws, was the other choice.
2. **Each call's event names its session:** yes, as one optional field on `agent_message`; logs without it read as today. With it each row of a loop's feed names its task where several work at once, and each task marks the paths its own sessions wrote, live. On a log without it a row names no task, and a path is marked only when its task is the node's one open task or its scope check ran.
3. **The ribbon's names:** each stretch is named by the part of its id before `@`, else by its first word split at `-`, where no stretch before it in declared order has that name; else by its last word under the same condition; else by its whole id. For fragua: `grill brief plan spec approve implement review alt conform fix lint fix-lint tests ship pr`, 95 cells, the design's own row. Ids alone, named from 134 columns, and a key in the workflow for a node's short name were the other choices.
4. **Arrows and the wheel move what is read; `j k` move a pointer:** apart wherever nothing is typed, as the desk already is, since the wheel arrives as arrows and has to scroll the history. Where a line is typed (a round, a new run, the palette) the arrows are the only pointer, so there the wheel moves the choice.
5. **A card's sentences, and every other phrase:** the design's, for every interface. Answered at the spec's approval as render's own, and turned by the user at the plan's approval. Render owns each phrase, so the CLI says the design's words wherever it says the same thing; where the spike made words up for its demo, the log's own stand in the design's form.
6. **The CLI's node page takes the pane's parts:** yes (what it hands over, a task's proof and scope, its feed's last rows), so a node says the same in a print and in the application. Its goldens change, reviewed. The run's page prints as it does.
7. **One module or three:** one, in phases.
8. **The setup's pack:** fetched over the network from the Yunta repository's `packs/fragua` at the binary's own version, a source the application names, behind the audit's confirmation; `yunta init` stays as it is.
9. **Keys that change:** as listed under "Keys": `a`, `n`, `q`, `f`, `enter`, the field's `esc` and the findings' `tab` change what they do, a run's `p` and `w` go, and the keys the design does not name stay, out of the legend.
10. **Departures a person sees:** the five stand. An answered finding keeps its row with a done mark. `esc` on a prompt puts it aside and stays on the run. The feed closes the pane. A run asking at its own terminal is never named in the attention row. No room is a standstill with collecting as its act.
11. **Times tick each second:** yes. A spinner and the plow still move with events alone.
12. **Screens that read the machine:** the machine and upkeep are read when opened and again on their key, never on a timer; every screen the log feeds is live.
