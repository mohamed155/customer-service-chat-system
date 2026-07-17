# Feature Specification: Prompt Management

**Feature Branch**: `018-prompt-management`

**Created**: 2026-07-16

**Status**: Draft

**Input**: User description: "Prompt Management — Manage prompts safely and version them. Scope: system prompt, prompt variables, prompt preview, prompt version history, restore previous version, prompt validation. Backend: store prompt versions, track who changed prompts, support rollback. Frontend: prompt editor, variables panel, preview panel, version history drawer. Acceptance: prompt changes create versions, users can view old versions, users can restore a version, prompt edits are audited."

## Clarifications

### Session 2026-07-16

- Q: How does prompt management relate to feature 017's inline prompt editor on the agent settings page? → A: 018 becomes the single prompt write path — the agent settings prompt section is replaced by (or navigates into) the prompt management editor, and every prompt save, wherever initiated, creates a version and passes validation.
- Q: Does saving immediately activate the new version, or is there a draft/publish split? → A: Save = activate — every validated save immediately becomes the active version; no draft state in v1.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every Prompt Change Becomes a Recoverable Version (Priority: P1)

A tenant administrator edits their AI agent's system prompt in the prompt editor and saves. Instead of overwriting the previous text, the save creates a new numbered version that becomes the active prompt, while every earlier version is preserved unchanged. The administrator can iterate on the prompt as often as they like, confident that no previous wording is ever lost.

**Why this priority**: Version-on-save is the safety foundation the entire feature exists for. Without it, history, restore, and audit have nothing to operate on — and a bad prompt edit is unrecoverable.

**Independent Test**: Save three successive edits to the system prompt, then verify three distinct versions exist, each retrievable with its exact saved content, and that the newest one is the prompt the AI agent actually uses.

**Acceptance Scenarios**:

1. **Given** a tenant with an existing system prompt, **When** an administrator edits the prompt text and saves, **Then** a new version is created with a higher version number, the new version becomes the active prompt, and the prior version remains retrievable with its original content intact.
2. **Given** a saved prompt version, **When** any later edit is saved, **Then** the earlier version's content, author, and timestamp are immutable — no operation in the feature can alter or delete a historical version.
3. **Given** an administrator saves the prompt without changing anything (content identical to the active version), **When** the save is submitted, **Then** the system informs them there are no changes and does not create a duplicate version.
4. **Given** a newly saved prompt version, **When** the AI agent next generates a customer reply, **Then** that reply is produced under the newly active version, not any earlier one.
5. **Given** two administrators editing the prompt at the same time, **When** the second one saves after the first, **Then** the second is warned that the prompt changed since they started editing and must review before their save is accepted — neither save silently overwrites the other's version.

---

### User Story 2 - Browse Version History and Restore a Previous Version (Priority: P1)

A tenant administrator discovers that a recent prompt change made the agent behave worse. They open the version history drawer, see the list of versions — each with its number, author, timestamp, and an optional change note — inspect the full content of any older version, and restore the one that worked. The restore takes effect immediately and is itself recorded as a new version, so history remains a complete, linear record.

**Why this priority**: Restore is the payoff of versioning — it turns a bad prompt deploy from an incident into a one-minute fix. It is co-critical with Story 1 (the two together are the minimum "safe prompt management" promise), but listed second because it consumes what Story 1 produces.

**Independent Test**: Create several versions, restore an older one, and verify (a) the agent now uses the restored content, (b) the restore appears in history as a new version referencing its source, and (c) all intermediate versions are still present and viewable.

**Acceptance Scenarios**:

1. **Given** a prompt with multiple versions, **When** an administrator opens the version history drawer, **Then** they see all versions in reverse chronological order, each showing version number, who saved it, when, and its change note if one was provided.
2. **Given** the version history drawer, **When** the administrator selects an older version, **Then** they can read that version's full prompt content and see how it differs from the currently active version.
3. **Given** an administrator viewing an older version, **When** they choose "Restore" and confirm, **Then** a new version is created whose content equals the restored version's content, that new version becomes the active prompt, and the history entry identifies it as a restore of the source version.
4. **Given** a completed restore, **When** the AI agent next generates a reply, **Then** it is produced under the restored content.
5. **Given** a prompt with a long history, **When** the administrator scrolls the history drawer, **Then** older versions load progressively and every version ever saved remains reachable.

---

### User Story 3 - Compose Prompts with Variables and Preview the Result (Priority: P2)

While editing, the administrator uses the variables panel to see the placeholder variables the platform supports (for example the agent's name, the tenant's business name, the customer's name, the conversation's channel) and inserts them into the prompt text. The preview panel shows the prompt exactly as it would be assembled for a real conversation, with each variable replaced by a realistic sample value, so the administrator can confirm the final wording before saving.

**Why this priority**: Variables and preview make prompts maintainable and trustworthy — the administrator sees what the AI will actually receive — but the feature is already safe and usable with plain-text versioning alone.

**Independent Test**: Insert two supported variables into the prompt, verify the preview renders them with sample values and updates as the text is edited, then save and confirm the stored version preserves the variable placeholders (not the sample values).

**Acceptance Scenarios**:

1. **Given** the prompt editor is open, **When** the administrator views the variables panel, **Then** they see every supported variable with its name, a plain-language description, and an example value.
2. **Given** the variables panel, **When** the administrator inserts a variable, **Then** the placeholder is added into the prompt text at the cursor position in the correct syntax.
3. **Given** a prompt containing variables, **When** the administrator looks at the preview panel, **Then** every variable is shown substituted with a sample value, and the preview updates to reflect the current unsaved editor content.
4. **Given** a prompt containing variables, **When** it is saved and later viewed in version history, **Then** the stored content contains the variable placeholders themselves, and the preview of any historical version renders with sample values the same way.

---

### User Story 4 - Invalid Prompts Are Blocked Before They Go Live (Priority: P2)

As the administrator types, the editor validates the prompt: unknown variable names, malformed placeholder syntax, an empty prompt, or a prompt exceeding the allowed length are flagged inline with a clear message pointing at the problem. A prompt that fails validation cannot be saved, so a broken prompt can never become the active version driving customer conversations.

**Why this priority**: Validation is the "safely" in "manage prompts safely" — it prevents the most common self-inflicted failure (a typo'd variable silently reaching customers) — but it guards the other stories rather than delivering standalone value.

**Independent Test**: Attempt to save a prompt containing a misspelled variable name and verify the save is rejected with a message identifying the offending placeholder; fix it and verify the save succeeds.

**Acceptance Scenarios**:

1. **Given** a prompt referencing a variable name that does not exist, **When** the administrator attempts to save, **Then** the save is rejected and the message identifies the unknown variable and where it appears.
2. **Given** a prompt with malformed placeholder syntax (for example an unclosed placeholder), **When** the administrator attempts to save, **Then** the save is rejected with a message pointing at the malformed fragment.
3. **Given** an empty prompt or a prompt over the maximum allowed length, **When** the administrator attempts to save, **Then** the save is rejected with a clear limit message and the current editor content is not lost.
4. **Given** a prompt with a validation problem, **When** the administrator is still editing, **Then** the problem is surfaced inline in the editor before they attempt to save, and the preview clearly marks the unresolved placeholder rather than rendering it as if valid.
5. **Given** validation is enforced when restoring, **When** an administrator restores an old version that references a variable the platform no longer supports, **Then** the restore is blocked with the same validation message flow instead of activating a broken prompt.

---

### User Story 5 - Prompt Changes Are Fully Audited (Priority: P3)

An owner or compliance reviewer needs to know how the agent's instructions have changed over time. Every prompt save and every restore records who performed it, when, and what changed. Reviewing the version history answers "who changed the prompt, when, and to what" for any point in the past.

**Why this priority**: Auditability is a hard platform requirement for AI configuration changes, but it is satisfied by recording metadata on the actions defined in Stories 1–2; it adds accountability rather than new user capability.

**Independent Test**: Have two different administrators each save a prompt change, then verify the history attributes each version to the correct person with the correct timestamp, and that the tenant's audit trail contains a corresponding entry for each save and restore.

**Acceptance Scenarios**:

1. **Given** any prompt save or restore, **When** it completes, **Then** an audit record is created capturing the acting user, the tenant, the action (save or restore), the affected version numbers, and the time.
2. **Given** the version history, **When** any version is viewed, **Then** it displays the identity of the person who created it, even if that person's account has since been deactivated.
3. **Given** audit records for prompt changes, **When** anyone attempts to modify or delete them, **Then** the records are unalterable — the audit trail is append-only.

---

### Edge Cases

- Two administrators edit the prompt concurrently: the later save must detect the conflict and require review, never silently discard the other's version (Story 1, scenario 5).
- A restore targets a version whose variables are no longer supported: validation blocks activation and explains why (Story 4, scenario 5).
- The author of a historical version has been removed from the tenant or deactivated: history still shows who they were.
- A save is submitted with content identical to the active version: no new version is created and the user is told why.
- The version history grows very large over years of edits: history remains complete and browsable without degrading the editor experience.
- A save fails partway (for example a connectivity drop): either a complete new version exists or nothing changed — the active prompt is never left in a partial state, and the administrator's unsaved editor content is preserved locally for retry.
- A tenant whose agent has never had its prompt configured opens prompt management: they see the starter/default prompt as the baseline rather than an error, and their first save creates version 1.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every successful save of the system prompt MUST create a new immutable prompt version rather than overwriting existing content; the new version becomes the tenant's active prompt.
- **FR-002**: Each prompt version MUST record its content, a monotonically increasing per-prompt version number, the identity of the user who created it, the creation time, an optional user-supplied change note, and — for versions created by restore — a reference to the source version.
- **FR-003**: Historical prompt versions MUST be immutable and retained indefinitely; no user-facing operation may edit or delete a stored version.
- **FR-004**: Users MUST be able to view the full list of a prompt's versions in reverse chronological order and read the complete content of any individual version.
- **FR-005**: Users MUST be able to see what changed between a selected historical version and the currently active version.
- **FR-006**: Users MUST be able to restore any historical version; a restore creates a new version with the restored content, marked as a restore of its source, and activates it — it never rewrites or truncates history.
- **FR-007**: The system MUST provide a defined catalog of supported prompt variables, each with a name, description, and sample value, and expose this catalog to the editing interface.
- **FR-008**: The prompt editor MUST let users insert catalog variables as placeholders in a single well-defined syntax.
- **FR-009**: The preview MUST render the current editor content with every variable substituted by its sample value, updating as the user edits, and MUST make unresolved or invalid placeholders visually unmistakable.
- **FR-010**: Prompt content MUST be validated before any version is created — on save and on restore alike. Validation MUST reject: references to variables not in the catalog, malformed placeholder syntax, empty content, and content exceeding the platform's maximum prompt length; each rejection MUST identify the specific problem and its location.
- **FR-011**: A prompt that fails validation MUST never become the active version, and a rejected save MUST NOT discard the user's in-editor content.
- **FR-012**: Concurrent edits MUST be detected: a save based on a version that is no longer the active one MUST be rejected with a conflict message requiring the user to review the newer version before resubmitting.
- **FR-013**: Saving content identical to the active version MUST NOT create a new version; the user MUST be informed no changes were detected.
- **FR-014**: Every prompt save and restore MUST produce an append-only audit record capturing the acting user, tenant, action type, affected version numbers, and timestamp, consistent with the platform's existing audit trail.
- **FR-015**: Prompt management MUST be restricted to the tenant roles permitted to manage AI agent settings (Owner and Admin); all other roles MUST have no access to edit, restore, or view prompt management screens.
- **FR-016**: All prompt versions, history, and audit records MUST be scoped to their tenant; no tenant may view or affect another tenant's prompts or history.
- **FR-017**: Once a version becomes active, all subsequent AI-generated replies for the tenant MUST be produced from that version until a newer version is activated.
- **FR-018**: The prompt management editor MUST be the platform's only write path for system prompt content. The agent settings page's inline prompt editing (feature 017) is superseded — its prompt section is replaced by or navigates into the prompt management editor — and no surface may modify prompt content without creating a validated version.

### Key Entities

- **Prompt**: The tenant's system prompt as a managed object — owned by the tenant's AI agent configuration; knows which of its versions is currently active. One system prompt per tenant agent in v1, with the shape allowing additional prompts per agent in the future.
- **Prompt Version**: An immutable snapshot of prompt content — version number, content with variable placeholders, author, creation time, optional change note, optional restored-from reference. Belongs to exactly one Prompt.
- **Prompt Variable**: A platform-defined placeholder available for use in prompt content — name, human-readable description, sample value for preview. Referenced (not embedded) by prompt content.
- **Prompt Audit Record**: An append-only record of a prompt change action — actor, tenant, action (save or restore), affected versions, timestamp. Lives in the platform's existing audit trail.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of successful prompt saves produce a distinct retrievable version; in testing, no sequence of edits, restores, or conflicts ever results in lost prompt content.
- **SC-002**: An administrator can find and restore a previous prompt version in under 1 minute from opening the version history, without assistance.
- **SC-003**: 100% of prompt saves and restores are attributable — for any version in history, "who, when, and what changed" is answerable from the product alone.
- **SC-004**: Zero invalid prompts (unknown variables, malformed placeholders, empty, over-length) can be activated through any path — editor save or restore — in testing.
- **SC-005**: In preview, 100% of supported variables render with sample values, and every unresolved placeholder is visibly flagged.
- **SC-006**: Concurrent-edit conflicts are surfaced to the later saver in 100% of tested overlap cases, with neither party's saved version silently lost.

## Assumptions

- **Scope of "prompts" in v1**: This feature manages the tenant AI agent's system prompt introduced by feature 017 (AI Agent Configuration). Per the session clarification, this feature's editor supersedes 017's inline prompt editing as the single write path for prompt content. Other prompt types (e.g., per-channel prompts, tool prompts) are out of scope but the versioning model must not preclude them.
- **Variable catalog is platform-defined**: v1 ships a fixed, platform-curated set of four variables — agent name, tenant business name, customer name, and conversation channel — exactly those the responder can resolve deterministically from data it already touches. Business hours is deliberately excluded: no tenant business-profile field exists anywhere in the schema (see research.md R4); it joins the catalog when such a field ships, as a one-constant change. Tenant-defined custom variables are out of scope for v1.
- **Restore is roll-forward**: Restoring never rewrites history; it creates a new version with the old content. This matches audit-trail expectations and keeps history strictly append-only.
- **Version retention**: All versions are retained indefinitely; no pruning in v1.
- **Access roles**: Mirrors feature 017's clarified decision — Owner and Admin manage prompts; Manager, Agent, and Viewer have no access.
- **Draft/publish workflow is out of scope** (confirmed in session clarification): v1 has a single "save = activate" flow (with validation and conflict checks). A separate draft state, approval workflow, or A/B testing of prompt versions is deferred.
- **Preview uses sample values only**: The preview substitutes curated sample values; it does not execute a live AI call or use real customer data.
- **Auditing reuses the platform audit trail**: Prompt change audit entries are recorded in the existing append-only audit mechanism established by feature 005 and used by features 006/017.
- **Dependency**: Requires feature 017's AI agent configuration (the prompt being managed) and the platform's existing authentication (007), RBAC (008), and audit (005) foundations.
