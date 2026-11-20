# Spec: Search, operation feedback, and list rendering

## Problem Statement

When browsing Azure DevOps variable groups, search behavior is split between state management and the action loop. Group searches leave non-matching groups in the list, while variable searches hide non-matches. Search edits, cache invalidation, and selection updates must be coordinated manually.

The same action loop also owns remote refresh, clipboard copy, and file export, but failures are surfaced differently. Finally, list rendering receives cloned domain entries and finds the selected row by comparing whole entries; when there is no selection, the main list area can appear blank.

## Solution

Make search behavior consistent and keep query, filtering, and selection updates together in the existing state owner. Give refresh, copy, and export clear and consistent user-visible outcomes. Simplify list rendering so it receives the already-filtered entries and the selected row position, and show an explicit message when a search has no matches.

Keep the current CLI workflow and Azure DevOps scope. This is a focused architecture improvement, not a rewrite or a new feature for editing variable groups.

## User Stories

1. As an Azure DevOps user, I want to browse the variable groups returned for my configured organization and project, so that I can inspect the configuration available in that scope.
2. As a user browsing groups, I want a fuzzy search to rank matching group names, so that I can find a group without typing its exact name.
3. As a user searching groups, I want non-matching groups removed from the results, so that the list contains only relevant matches.
4. As a user viewing a variable group, I want to search variable names, so that I can quickly find a particular entry.
5. As a user viewing a variable group, I want to search visible non-secret values as well as names, so that I can find an entry by a value I know.
6. As a user, I want secret entries to be searchable by name but not by their redaction placeholder, so that hidden values do not create misleading matches.
7. As a user, I want the highest-ranked result selected while I type, so that I can immediately inspect or navigate from the best match.
8. As a user, I want the selection cleared when no results match, so that an unrelated entry is not treated as selected.
9. As a user, I want an explicit “No matches” message when a search returns nothing, so that an empty list is not mistaken for a rendering failure.
10. As a user, I want group search text retained when I leave search mode, so that I can continue browsing the filtered list.
11. As a user, I want each variable group to retain its own variable search text, so that returning to a group restores my previous search.
12. As a user, I want navigation keys to move only among the currently filtered results, so that I cannot select a hidden entry.
13. As a user, I want a refresh to preserve the selected group and variable by name when they still exist, so that refreshing does not unnecessarily disrupt my place.
14. As a user, I want refresh to select a sensible first result if my former selection disappeared, so that I can continue browsing after remote changes.
15. As a user, I want refresh failures shown in the status area while the previously loaded snapshot remains available, so that a temporary remote failure does not erase useful data.
16. As a user, I want copying a selected variable to report success or failure in the status area, so that I know whether the requested operation completed.
17. As a user, I want exporting the selected variable group to report success or failure in the status area, so that I know whether the file was written.
18. As a user, I want copy and export requests with no applicable selection to produce a clear status message, so that these requests do not fail silently.
19. As a user, I want exported secret entries to contain only the existing redacted placeholder and secret marker, so that export does not reveal protected values.
20. As a user, I want the existing keyboard navigation, theme choice, and group/variable views to continue working, so that these improvements do not disrupt the established workflow.

## Implementation Decisions

- The existing state owner is responsible for query changes, filtered results, cache validity, and selection synchronization. The action loop translates actions and delegates these related rules instead of mutating queries and invalidating caches separately.
- Group and variable searches both omit non-matching results and order matches by fuzzy-match score.
- Variable searches match names and visible non-secret values. Secret entries are matched by name only.
- Group search text remains shared across the group list. Variable search text remains separate for each variable group.
- Search results update as the query changes. The first ranked result becomes selected; an empty result set has no selected entry.
- Leaving search mode retains the query. Refresh attempts to preserve selections by name, falling back to the first available result when necessary. The selected entry must still be a member of the filtered results.
- The screen drawing code prepares the filtered rows and selected row position. List widgets render the supplied selection position rather than locating a selected row by comparing whole domain entries.
- Both group and variable lists show an explicit empty-result state when a non-empty search has no matches. Search and state code does not construct presentation text.
- Refresh, copy, and export outcomes are surfaced through the existing status area. Failures use error status messages; successful copy and export report completion. A successful refresh clears the prior failure status.
- The existing Azure DevOps client interface and production adapter remain the remote-data seam. Do not add a generic effects framework.
- A clipboard seam is justified for deterministic tests: the operating-system clipboard is the production adapter and a test fake is the second adapter. Keep file export testable with a temporary directory rather than adding a filesystem interface.
- Export remains pretty-printed JSON at the existing default filename. Secret values remain redacted.

## Testing Decisions

- Tests assert externally observable behavior through the highest practical seam; they do not assert filter-cache internals or depend on the names of private helper methods.
- Exercise query edits, filtered results, selection changes, refresh preservation, and status updates through the existing state-action channel and its emitted state snapshots.
- Use the existing fake Azure DevOps client pattern to test refresh success and failure.
- Use a fake clipboard adapter to verify copy success and failure outcomes without relying on a desktop clipboard.
- Use a temporary directory to verify exported JSON and export failure outcomes.
- Use Ratatui's test backend to render group and variable lists, verifying selected-row placement and explicit empty-result messages.
- Existing state and action-loop tests provide prior art for state behavior, refresh results, and selection; existing temporary-directory export tests provide prior art for filesystem output.

## Out of Scope

- Editing, creating, or deleting Azure DevOps variable groups or variable entries.
- Changing authentication, configuration, Azure DevOps permissions, or the remote data format.
- Adding persistence for search queries or UI state between CLI runs.
- Introducing a general-purpose operations framework or new adapters for Azure DevOps and the filesystem.
- Changing keyboard shortcuts, the available themes, or the CLI command structure.

## Further Notes

- This spec synthesizes the three candidates in the architecture review into one cohesive change.
- The behavior details above are recommended defaults based on the current implementation and the review; they are written as the proposed contract for review before implementation.
- The user confirmed the proposed test approach: exercise actions through the app's message channel, verify terminal output with a test backend, and use test substitutes for clipboard and file output.
- No architecture decision record is proposed: these choices are local, reversible, and do not involve a significant long-term trade-off.
