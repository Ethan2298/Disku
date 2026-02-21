# Project Instructions

## Commit Rules

When committing (especially via `/commit`), follow these rules:

### Split vs Single Commit
- **Split** when changes span multiple concerns (e.g., feature + refactor, new code + tooling)
- **Keep as one** when all changes serve a single purpose and splitting would break the build
- **Litmus test**: If you need "and" to describe the commit, split it

### Commit Message Format
Use Conventional Commits with imperative mood:
```
<type>(<scope>): <subject>

<body — explain WHY, not what>

Co-Authored-By: ...
```

Types: `feat`, `fix`, `refactor`, `perf`, `test`, `chore`, `docs`, `style`

### Rules
- Every commit must compile
- Never mix formatting with logic changes
- Subject line: imperative mood, 50 chars max, no period
- Body: wrap at 72 chars, explain motivation not mechanics
