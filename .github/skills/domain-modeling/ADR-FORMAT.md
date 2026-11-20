# ADR Format

ADRs live in `docs/adr/` and use sequential numbering: `0001-slug.md`, `0002-slug.md`, and so on. Create the directory only when the first ADR is needed.

## Template

```md
# {Short title of the decision}

{1–3 sentences: what's the context, what did we decide, and why.}
```

An ADR can be a single paragraph. The value is in recording that a decision was made and why, not in filling out sections.

## Optional sections

Include these only when they add genuine value:

- **Status** frontmatter (`proposed | accepted | deprecated | superseded by ADR-NNNN`), useful when decisions may be revisited.
- **Considered Options**, when rejected alternatives are worth remembering.
- **Consequences**, when non-obvious downstream effects need to be called out.

## Numbering

Scan `docs/adr/` for the highest existing number and increment by one.

## When to offer an ADR

All three must be true: the decision is hard to reverse; a future reader would wonder why it was made; and there was a genuine trade-off with alternatives. Examples include architectural shape, integration patterns between contexts, technology choices with significant lock-in, boundaries and scope, deliberate deviations from the obvious path, constraints not visible in code, and non-obvious rejected alternatives.
