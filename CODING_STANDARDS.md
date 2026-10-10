# Coding Standards

Rules for code written in this repository. Agent behavior rules live in `AGENTS.md`. Extend this file as the repo's conventions crystallize.

## Comments

- The code is the source of truth; a comment never is. When a comment and the code disagree, the code wins: fix the comment or delete it.
- Default to no comments for what the code already says — names and structure should be the explanation.
- Comment only what the code cannot express, as a constraint that still binds: hidden constraints, counterintuitive behavior, deliberate deviations, compatibility requirements. When the code changes, update or delete the comment; keep no comment that describes code as it once was.
- Document exported API — symbols other code or packages consume — with doc comments that state the contract (purpose, invariants, units, thread-safety, caller obligations).
- Trade-off rationale and rejected alternatives live in ADRs, not in code comments.

## Error handling

- Handle errors, or propagate them with context (what failed, with what input); silently swallowing an error is a defect. Let tooling flag the mechanical forms (empty `catch`, ignored error returns).
- Errors surfaced to users never leak internals — stack traces, internal paths, queries, secrets. Return a generic message and log the detail.

## Resources

- Acquired resources (handles, connections, locks, temp files) are released on every path, including error paths; prefer scoped owners (`defer`, `with`, RAII, `Drop`) over manual release.

## Tests

- Behavior ships with a test that fails without it; a test that always passes is waste.

## Consistency

- Match the convention of the file you are editing; surrounding style settles style debates, not taste.

## Concurrency

- Shared mutable state has one owner or one synchronization mechanism; keep critical sections minimal, and never hold a lock across blocking work.

## Out of scope

- Updating or merging an existing standards file: the human decides what changes.
- Writing ADRs: rationale belongs in ADRs, and the doc points there.

<!-- end of baseline v1; repo-owned content below survives baseline updates -->

## Comments: repo rules

The policy above is the baseline. The rules below make it concrete for this repository: what passes review, and what gets deleted. They govern comments you add or touch; they do not demand a sweeping rewrite of existing comments — do that separately and deliberately, if at all.

### The one test

Delete the comment in your head and re-read the code. If a reader would still write the correct code, the comment is noise. A comment must carry something the code cannot: a precondition, an invariant, a side effect, or the reason a plausible-looking simplification would be wrong.

### Allowed

- A doc comment for an item whose contract is not visible in its name and signature: panics, error and degradation behavior.
- A short in-line pointer to where the full reasoning lives: `ADR-0012`, `GLOSSARY.md「忙碌门禁」`, `#26`. Cite it; do not reproduce it in the source.

### Not allowed

- `TODO` / `FIXME` with no owner and issue. New ones are `TODO(#NN): …`; never add a bare one.
- A doc comment on every field, variant, or constant by reflex. Document the type once, then only the members whose meaning the name does not carry.

### Doc comments

- `//!` at the top of a module states what the module is for and what its public interface is; `///` on an item states its contract. They are written for the caller, not as notes to the implementer.
- Do not duplicate a type's own doc onto its `impl` blocks, accessors, or self-explanatory fields.

### Deleting or rewriting comments

- Edit comments whole-sentence. Delete a doc block entirely, or keep only lines that form complete, self-contained sentences. Never leave orphaned continuation lines — a leftover that starts with `或` / `（` / `、` or that lost its subject (`/// 是否为空都不能落「已是最新」` without the `文件缺失时无论线上版本` lead-in). After deleting, re-read every kept line and fold fragments back into complete sentences.
- Rewrite the whole sentence when deleting its premise would change its meaning. `向导步骤 2 拒绝空路径，设置页允许空，启动回退视为未设置` must not shrink to `向导步骤 2 拒绝空路径…共用同一口径` — the surviving text would now contradict the code (all subjects read as rejecting empty paths).
- Never delete load-bearing contracts: `SAFETY:` notes on `unsafe` blocks, decision tables/matrices, `panic`/error/idempotency semantics, compatibility and fallback constraints (`空串 = 未设置`, `缺省启用`), and ADR/issue/GLOSSARY pointers.
- When trimming history (`此前…`, `#NN 回归修复` labels), drop the narration but keep the constraint and the pointer it carried.
- A comment-cleanup diff touches comments only: no code edits and no reformatting of neighboring code.

### Language

- Comments and doc comments in this repo are written in Chinese — the established repo convention, which takes precedence over the English-comments default in the global `AGENTS.md`. Identifiers, CLI output, and commit messages stay in English.