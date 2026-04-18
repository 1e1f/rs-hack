# /comment — Post a summary to the hack-board

Write your current work summary to `.hack/summaries/` so it shows up on the hack-board and can be promoted to a relay.

## What to do

1. **Figure out what ticket you're working on**: Check the current file for `@hack:ticket(...)` or `@hack:relay(...)` annotations. If you find one, use its ID — including the compound form for sub-tickets (e.g. `R007-T1`). If you're not sure, skip it — orphan summaries go to the board inbox.

2. **Write the summary**: Use the `hack_summary` MCP tool (or `rs-hack board summary` CLI). Include:
   - What you did in this session
   - What's left / what's blocking
   - Any gotchas or surprises for the next person
   - Keep it natural — markdown, bullet points, whatever feels right

   ```
   hack_summary(
     text: "your summary here — multi-line markdown is fine",
     ticket: "R007-T1",      // optional: bare (T03, R012) or compound (R007-T1)
     author: "agent:claude"  // optional
   )
   ```

3. **Check for architecture docs**: If you created or modified any markdown files in `architecture/`, make sure the ticket has an `@arch:see(...)` linking to them. If not, add one:

   ```rust
   //! @arch:see(architecture/path/to/your_doc.md)
   ```

4. **Confirm**: Tell the user the summary ID and where it was written.

## Tips

- This is the low-friction version of `/handoff`. Use `/comment` when you want to record progress. Use `/handoff` when you're done and the next agent needs a structured baton.
- Don't overthink formatting. The board and humans can promote a comment to a relay later.
- If you wrote a longer doc (like a `REFACTOR_CONTEXT.md` or `MIGRATION_PLAN.md`), just reference it: "See ./REFACTOR_CONTEXT.md for full details" — the board can inline it.
