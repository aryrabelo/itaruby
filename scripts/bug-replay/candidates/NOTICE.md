# NOTICE — embedded third-party diff hunks

The `candidates/<repo>.jsonl` records in this directory embed short,
verbatim diff hunks (`removed[]`/`added[]` lines) mined from the git
history of three public repositories, together with a bug/fix pair
commit subject:

| repo | license | upstream license link |
|---|---|---|
| rails/rails | MIT | <https://github.com/rails/rails/blob/main/MIT-LICENSE> |
| mastodon/mastodon | AGPL-3.0 | <https://github.com/mastodon/mastodon/blob/main/LICENSE> |
| discourse/discourse | GPL-2.0 | <https://github.com/discourse/discourse/blob/main/LICENSE.txt> |

Every record attributes the excerpt to its exact upstream source: the
`fix_sha`/`parent_sha` commit shas plus the `files`/`hunks[].path`
paths identify the precise commits and files the lines come from.

Purpose: these excerpts are the benchmark corpus of the bug-replay
harness (`scripts/bug-replay/`) — a regression-replay benchmark that
checks a type checker at the buggy parent revision and at the fix
revision of real, historical bugs. The hunks are minimal excerpts used
solely to identify and replay each pair, not reused as functional code.

The upstream projects retain all rights over their respective code;
their names and trademarks belong to them, and their inclusion here
implies no endorsement.
