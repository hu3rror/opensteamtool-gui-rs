# Coding Standards

Rules for code written in this repository. Agent behavior rules live in `AGENTS.md`; trade-off rationale and rejected alternatives live in ADRs. Extend this file as the repo's conventions crystallize.

## Comments

- Default to no comments. Let names and structure make the code self-explanatory.
- Write comments only for reasons the code itself cannot express: hidden constraints, counterintuitive behavior, historical pitfalls, and special compatibility requirements.
- Put trade-off rationale and rejected alternatives in ADRs, not in code comments.

## Comments: repo rules

The policy above is the baseline. The rules below make it concrete for this repository: what passes review, and what gets deleted. They govern comments you add or touch; they do not demand a sweeping rewrite of existing comments — do that separately and deliberately, if at all.

### The one test

Delete the comment in your head and re-read the code. If a reader would still write the correct code, the comment is noise. A comment must carry something the code cannot: a precondition, an invariant, a side effect, or the reason a plausible-looking simplification would be wrong.

### Allowed

- A doc comment on an item whose contract is not visible in its name and signature: invariants, panics, error and degradation behavior, or special compatibility requirements.
- An implementation comment for a hidden constraint, counterintuitive behavior, historical pitfall, or compatibility requirement — state the reason, not the mechanics.
- A short in-line pointer to where the full reasoning lives: `ADR-0012`, `GLOSSARY.md「忙碌门禁」`, `#26`. Cite it; do not reproduce it in the source.

### Not allowed

- Restating the code: narrating the next statement (`// 启动 → 隐藏。`), summarizing the function body, or echoing the signature and parameter names.
- Section and banner dividers (`// ---------- 派生函数 ----------`, `// ==================== 状态机转移 ====================`). Extract a function, module, or file instead.
- Commented-out code. Git keeps the history; delete it.
- Changelog and provenance narration in source (`此前…`, `旧实现…`, `自 ui.rs 迁入`, `previously…`, `moved from…`). The commit message, ADR, or `GLOSSARY.md` owns that.
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
