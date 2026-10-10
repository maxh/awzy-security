---
name: write-adr
description: Write an architecture decision record in docs/adr/. Use when a change makes or reverses an architectural or product decision (a new service, a protocol, a data store, a security boundary, a deployment model), or when asked to record a decision.
---

# Write an ADR

Decisions live in `docs/adr/`, one numbered file each. There's no index table: `ls docs/adr` is the
index.

1. **Check it isn't already decided.** Search the records (`grep -ril '<topic>' docs/adr`). If an accepted ADR covers it, don't contradict it
   silently: either follow it, or write a new ADR that supersedes or amends it.
2. **Number and name it.** Take the next number after the highest in `docs/adr/`
   (`ls docs/adr | sort | tail -3`). File name: `NNNN-short-kebab-title.md`, matching the title.
3. **Use the house shape**, and read the two most recent records to match their tone and length:

   ```markdown
   # NNNN. Title

   - Status: Accepted
   - Date: YYYY-MM-DD
   - Implements part of: [0003](0003-one-pdp-for-every-access-path.md) (optional; why it relates)

   ## Context
   ## Decision
   ## Consequences
   ```

   Context is what forced the decision; Consequences include what it costs.
4. **Write plainly.** Short sentences, concrete names (services, env vars, files), and the
   alternatives that were rejected with a sentence on why.
5. **Number it** after the highest record on `main` (`git fetch && git ls-tree --name-only
   origin/main docs/adr/ | tail -1`). If another branch takes the same number first, renumber
   yours before it merges.
6. **Superseding or amending:** accepted records aren't edited, except their status. Set the old
   record's status line to name the new one (e.g. `Accepted; partly superseded by [0016](…)`).
7. **Link it** from the code or docs it governs (a module doc comment, `DEVELOPMENT.md`, the
   platform plan) as "ADR NNNN", the way existing code does.
