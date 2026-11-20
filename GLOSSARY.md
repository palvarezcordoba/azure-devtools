# Azure DevTools

Azure DevTools is a CLI for inspecting Azure DevOps variable groups within a configured organization and project.

## Language

**Azure DevOps Scope**:
The organization and project combination that identifies the variable groups available in a session.
_Avoid_: Environment, tenant

**Variable Group**:
A named collection of variable entries belonging to an Azure DevOps project.
_Avoid_: Variables, configuration

**Variable Entry**:
A named value within a variable group. An entry may represent a secret.
_Avoid_: Setting, property

**Secret Variable Entry**:
A variable entry whose value is protected and not revealed by Azure DevTools.
_Avoid_: Password, credential

**Variable Group Snapshot**:
A point-in-time collection of variable groups fetched for an Azure DevOps scope.
_Avoid_: Cache, local copy
