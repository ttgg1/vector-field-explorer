# Review records

Review records live in `docs/reviews/` and are maintained by the controller after review. Each Markdown file describes a ticket, review date, branches, merge, and a **Commits** table containing backticked commit hashes (full or abbreviated) and subjects. The `**Ticket:**` line must match the ticket number in each covered commit subject.

CI runs `scripts/check-review-records.py` independently of Rust gates. For commits newer than the enforcement baseline documented in that script, any subject containing `(ticket NN)` must be named by a record with that ticket number. Commits at or before the baseline are grandfathered.
