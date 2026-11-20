# GLOSSARY.md Format

## Structure

```md
# {Context Name}

{One or two sentence description of what this context is and why it exists.}

## Language

**Order**:
{A one or two sentence description of the term}
_Avoid_: Purchase, transaction
```

## Rules

- **Be opinionated.** Choose the best term for a concept and list alternatives under `_Avoid_`.
- **Keep definitions tight.** Use at most one or two sentences. Define what the term is, not what it does.
- **Include only terms specific to this project's context.** General programming concepts do not belong.
- **Group terms under subheadings** when natural clusters emerge; otherwise use a flat list.

## Single- vs multi-context repositories

For a single context, keep one root `GLOSSARY.md`. For multiple contexts, use a root `GLOSSARY-MAP.md` to list contexts, their glossary locations, and relationships. The map can also describe how contexts communicate or share concepts.

Infer the structure from existing files: read `GLOSSARY-MAP.md` if present; otherwise use a root glossary if one exists. If neither exists, create a root `GLOSSARY.md` lazily when the first term is resolved. If multiple contexts exist and the relevant one is unclear, ask.
