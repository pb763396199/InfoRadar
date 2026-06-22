# Data Policy

InfoRadar v1 is designed for static, read-only public publishing.

Public exports may contain:

- title
- canonical URL
- source name
- board and category
- published or collected time
- short description or evidence snippets
- system score and reason
- duplicate/source count

Public exports must not contain:

- raw HTML
- full article body copied from source sites
- API tokens or credentials
- internal error stacks
- private comments or logs
- non-whitelisted raw source fields

Original observations and operational logs should stay in the local SQLite
database or private CI artifacts.
