# GitHub Issues: Creating and Linking with DESIGN.md

## Overview

GitHub issues are the **primary tracking mechanism** for features, enhancements, and bugs in the Galactic Market Simulator. To maintain architectural coherence, **all new issues must be linked to relevant DESIGN.md sections** and include explicit dependency relationships with other issues.

This ensures:
- ✅ Developers always understand the architectural context
- ✅ Dependencies are visible upfront (no surprise blockers)
- ✅ Development roadmap stays synchronized with design
- ✅ Future Copilot agents can see the full picture

---

## Issue Linking Standard

### 1. **Template: GitHub Issue Body Structure**

Every GitHub issue should follow this structure:

```markdown
## Description
[One-paragraph summary of the feature/bug/enhancement]

**Related DESIGN.md Sections:**
- §X.Y: [Section title and brief description]
- §A.B: [Section title and brief description]

[Detailed problem statement, motivation, or technical background]

## Proposed Solution / Approach
[How to implement this feature; technical approach; design decisions]

## Success Criteria
- [ ] Criterion 1
- [ ] Criterion 2
- [ ] ...

## Dependencies & Blockers
**Depends On:** Issue #X, Issue #Y (if applicable)
**Blocks:** Issue #Z (if applicable)
**Related To:** Issue #A (if applicable but not blocking)

[Additional context specific to this issue type]
```

### 2. **Locating Relevant DESIGN.md Sections**

When creating or updating an issue, identify which sections of DESIGN.md are most relevant:

#### By Feature Area:

| Area | DESIGN.md Sections |
|------|-------------------|
| **Economic Cycle** | §3.2 (Price Discovery), §3.4 (Company Lifecycle), §5.4 (Finance) |
| **Population/Consumption** | §3.3 (Supply & Demand), §3.1a (Food Production) |
| **Banking/Finance** | §5.3 (Company AI), §8: Finance Phase (Phase 7) |
| **Resources/Production** | §3.1 (Production Chains), §4.1 (Seeding), §4.2 (Database Tables) |
| **Geography/Terrain** | §2.2 (Resource Geography), §3.1a (Plantations) |
| **Events/Politics** | §6 (Political Simulation), §7 (Random Events), §8.2–8.4 (Roadmap) |
| **Naming/UI** | §5.2 (Project Structure), §8: Stage 5–6 (Web UI, God Mode) |

#### Finding Sections Automatically:

1. Use `grep` to search DESIGN.md for relevant keywords
2. Identify the section number (§X.Y) and title
3. Include 2–4 most relevant sections in the issue

### 3. **Specifying Dependencies**

Dependencies are **critical** for unblocking development. Use this format:

```markdown
## Dependencies & Blockers

**Depends On:** 
- Issue #9 (must complete before this issue)
- Issue #11 (blocks this issue)

**Blocks:** 
- Issue #10 (this issue must complete before #10)
- Issue #12 (this issue must complete before #12)

**Related To:** 
- Issue #5 (related but not blocking)
```

**Key Distinction:**
- **Depends On:** This issue cannot start until the other issues are done
- **Blocks:** This issue must be done before other issues can proceed
- **Related To:** Related work; useful context but not a hard blocker

### 4. **Success Criteria Format**

Success criteria should be specific, testable, and tied to the issue's goal:

```markdown
## Success Criteria
- [ ] [Specific, testable outcome 1]
- [ ] [Specific, testable outcome 2]
- [ ] All existing tests pass; no performance regression
- [ ] DESIGN.md updated if architectural changes are made
```

**Guidelines:**
- Each criterion should be objectively verifiable
- Include "tests pass" and "documentation updated" if applicable
- Avoid vague criteria like "feature is complete"

---

## Issue Types & Examples

### Type 1: Feature Enhancement

**Example: Issue #10 (Dynamic Population & Migration)**

```markdown
## Description
Implement a dynamic population and labor system to close the economic loop 
and introduce political-economic consequences.

**Related DESIGN.md Sections:**
- §3.3: Supply & Demand Model
- §6.2: Political Event Types (Population Migration)
- §8: Development Roadmap (Stage 2.5→3)

## Proposed Solution
...

## Success Criteria
- [ ] Population grows/shrinks dynamically based on food supply
- [ ] Citizens can only consume what they have earned through labor
- [ ] Migration shifts labor pools toward productive/safe sectors
- [ ] All tests pass; no performance regression

## Dependencies & Blockers
**Depends On:** Issue #9 (Balanced Economic Cycle)
**Blocks:** Stage 4 War Mechanics (war must have economic consequences)
```

### Type 2: Infrastructure/Refactoring

**Example: Issue #11 (Dynamic Resource Loading via JSON)**

```markdown
## Description
Migrate hardcoded resources to a JSON configuration file to enable easy 
expansion without bugs.

**Related DESIGN.md Sections:**
- §4.1: Database Design (Seeding)
- §4.2: Core Table Groups (Resource & Production Tables)

## Proposed Solution
...

## Success Criteria
- [ ] All resources load from data/resources.json
- [ ] Validation ensures recipe inputs exist
- [ ] Integration tests cover malformed JSON scenarios
- [ ] DESIGN.md updated with resource configuration format

## Dependencies & Blockers
**Unblocked:** Can be started anytime
**Blocks:** Issue #12 (Terrain-based Fertility) — needs flexible resources
```

### Type 3: Bug Fix

**Example: [Hypothetical Bug Fix]**

```markdown
## Description
[Bug description and impact]

**Related DESIGN.md Sections:**
- §X.Y: [Relevant section]

## Root Cause
[Why this bug exists; reference to design/implementation]

## Proposed Fix
[Technical solution]

## Success Criteria
- [ ] Bug is fixed
- [ ] Root cause prevented with tests
- [ ] No regressions in related systems

## Dependencies & Blockers
**Related To:** Issue #X (if relevant)
```

---

## Creating a New Issue: Step-by-Step Workflow

### Step 1: Identify the Scope
- Does this feature fit into an existing Stage? (§8 Roadmap)
- Does it depend on or block other features?
- What DESIGN.md sections does it relate to?

### Step 2: Check Existing Issues
- Use `github-mcp-read-write-list_issues` to see current issues
- Verify no duplicate exists
- Identify blockers/related issues

### Step 3: Map to DESIGN.md
```bash
# Search DESIGN.md for relevant sections
grep -n "keyword1\|keyword2" DESIGN.md

# Identify section numbers (§X.Y format)
# Add 2–4 most relevant sections to issue body
```

### Step 4: Write Issue Body
Use the template from **§1** above. Include:
- Clear description
- 2–4 DESIGN.md section references
- Proposed approach
- Specific success criteria
- Dependency information

### Step 5: Update DESIGN.md (if needed)
If the issue describes a **new roadmap item**, add it to §8:

```markdown
#### Stage X — [Title]
- ⏳ [Item description] (tracked in issue #N)
```

### Step 6: Create Issue in GitHub
Use `github-mcp-read-write-issue_write` with `method: "create"`:

```powershell
# Example (pseudo-code)
github-mcp-read-write-issue_write(
    method="create",
    owner="dealien",
    repo="Galactic-Market",
    title="Feature: [Title]",
    body="[Full issue body with DESIGN.md references]",
    labels=["enhancement", "stage-3"],  # if applicable
)
```

### Step 7: Update Issue Mapping
Add entry to SQL table:

```sql
INSERT INTO issue_design_mapping (issue_number, design_section, stage, priority, blocking_issues, notes)
VALUES (NEW_ISSUE_NUMBER, '§X.Y, §A.B', 'stage', 'priority', 'issue-Z', 'description');
```

---

## Updating an Existing Issue: Step-by-Step

### When to Update
- Design document changed; issue needs synchronization
- Blockers/dependencies discovered; add them
- Scope clarification needed
- New implementation details emerged

### How to Update

1. **Pull Latest Issue Details**
   ```powershell
   github-mcp-read-write-issue_read(
       method="get",
       owner="dealien",
       repo="Galactic-Market",
       issue_number=ISSUE_NUM
   )
   ```

2. **Modify Body with Updates**
   - Add new DESIGN.md sections if discovered
   - Update dependencies if blockers emerged
   - Refine success criteria if scope changed

3. **Apply Update**
   ```powershell
   github-mcp-read-write-issue_write(
       method="update",
       owner="dealien",
       repo="Galactic-Market",
       issue_number=ISSUE_NUM,
       body="[Updated body]"
   )
   ```

4. **Update SQL Mapping**
   ```sql
   UPDATE issue_design_mapping 
   SET design_section='§X.Y, §A.B', blocking_issues='issue-Y'
   WHERE issue_number=ISSUE_NUM;
   ```

---

## Copilot Agent Workflow

When a Copilot agent is assigned to work on an issue:

1. **Read the Issue**
   - Understand the feature/bug description
   - Note all dependencies and blockers
   - Identify DESIGN.md sections

2. **Read Referenced DESIGN.md Sections**
   - Use `view` tool to read relevant sections
   - Understand architectural context and constraints
   - Verify scope against design

3. **Check Blockers**
   - Confirm all "Depends On" issues are marked "done" in GitHub
   - If not, ask for clarification or defer work

4. **Update Issue Status**
   - Leave comment: "Starting work on issue #N"
   - Update checklist as progress is made

5. **At Completion**
   - Ensure all success criteria are met
   - Update DESIGN.md if architectural changes made
   - Link PR to issue with `Closes #N` in commit message
   - Leave final comment with summary

---

## Example: Creating a New Issue from Scratch

### Scenario
You want to add a new feature: **"Implement Trade Route Visualization"** (hypothetical future Stage 5 feature)

### Step 1: Identify Scope
- **Stage:** 5 (Web UI)
- **Dependency:** Requires Issue #11 (JSON Resources) done first
- **Related:** Issue #3 (REST API design)

### Step 2: Check Existing Issues
```powershell
# Use list_issues to verify no duplicate
github-mcp-read-write-list_issues(owner="dealien", repo="Galactic-Market", state="OPEN")
# → No existing issue for trade visualization
```

### Step 3: Map to DESIGN.md
```bash
grep -n "trade\|logistics\|visualization" DESIGN.md
# → Lines mentioning:
#   - §2.3: Transport & Logistics
#   - §3.2: Price Discovery (arbitrage)
#   - §5.2: Logistics module
#   - §8.3: Stage 5 Web UI
```

### Step 4: Write Issue Body

```markdown
## Description
Add a visual representation of active trade routes on the galactic map, showing 
cargo flows and economic interdependencies between systems.

**Related DESIGN.md Sections:**
- §2.3: Transport & Logistics
- §5.2: Project Structure (logistics.rs module)
- §8: Development Roadmap (Stage 5 — Web UI)

## Proposed Solution
1. Extend REST API with `/trade-routes` endpoint (GET all active routes)
2. Frontend: D3.js force-directed graph showing trade flows
3. Color-code by resource type; width by volume
4. Click route to see details (origin, destination, ETA, cargo)

## Success Criteria
- [ ] REST endpoint `/GET /trade-routes` returns route data
- [ ] Frontend renders D3 graph with routes
- [ ] Route details popup works on click
- [ ] Performance acceptable with 1000+ routes
- [ ] All tests pass; no regressions

## Dependencies & Blockers
**Depends On:** 
- Issue #11 (JSON Resources) — for flexible resource system
- Issue #3 (REST API) — for API infrastructure

**Related To:**
- Issue #5 (Banks & Banking) — economic flows visualized
```

### Step 5: Create Issue
```powershell
github-mcp-read-write-issue_write(
    method="create",
    owner="dealien",
    repo="Galactic-Market",
    title="Feature: Trade Route Visualization on Galactic Map",
    body="[Body from Step 4]",
    labels=["enhancement", "stage-5"]
)
# → Returns issue #15
```

### Step 6: Update SQL Mapping
```sql
INSERT INTO issue_design_mapping (issue_number, design_section, stage, priority, blocking_issues, notes)
VALUES (15, '§2.3, §5.2, §8.3', '5', 'medium', 'issue-11,issue-3', 'Trade route visualization for web UI');
```

### Step 7: (Optional) Update DESIGN.md
Add to §8 (Roadmap):
```markdown
**Stage 5 — Web UI:**
- ⏳ Trade route visualization on galactic map (tracked in issue #15)
```

---

## Best Practices

### DO ✅
- Link every issue to 2–4 DESIGN.md sections
- Explicitly list all blockers and dependencies
- Write specific, testable success criteria
- Update DESIGN.md if adding new roadmap items
- Include stage number in issue title or labels
- Use SQL mapping table to track issue → DESIGN.md relationships

### DON'T ❌
- Create issues without referencing DESIGN.md sections
- Leave dependencies/blockers undocumented
- Write vague success criteria ("feature is done")
- Modify DESIGN.md without updating related issues
- Create duplicate issues without checking existing ones
- Orphan issues (every issue should map to a stage/section)

---

## SQL Tracking: `issue_design_mapping` Table

### Schema
```sql
CREATE TABLE issue_design_mapping (
    issue_number INTEGER PRIMARY KEY,
    design_section TEXT,              -- e.g., "§3.2, §5.4"
    stage VARCHAR(10),                -- e.g., "2", "3", "2.5"
    priority VARCHAR(20),             -- "critical", "high", "medium", "low"
    blocking_issues TEXT,             -- e.g., "issue-9, issue-11"
    notes TEXT
);
```

### Queries

**Find all issues by priority:**
```sql
SELECT * FROM issue_design_mapping WHERE priority='critical' ORDER BY issue_number;
```

**Find blockers for an issue:**
```sql
SELECT * FROM issue_design_mapping WHERE issue_number=10;
-- blocking_issues = "issue-9" means issue #10 depends on issue #9
```

**Find all issues in a stage:**
```sql
SELECT * FROM issue_design_mapping WHERE stage='3' ORDER BY priority DESC;
```

**Identify critical path (issues that block others):**
```sql
SELECT issue_number, design_section FROM issue_design_mapping 
WHERE blocking_issues LIKE CONCAT('%issue-', issue_number, '%')
ORDER BY priority DESC;
```

---

## Summary Checklist

When creating/updating a GitHub issue, ensure:

- [ ] **Description** is clear and well-motivated
- [ ] **DESIGN.md sections** are referenced (2–4 sections)
- [ ] **Dependencies** are listed ("Depends On", "Blocks", "Related To")
- [ ] **Success criteria** are specific and testable
- [ ] **Stage** is identified (1–6 or feature area)
- [ ] **Priority** is set (critical/high/medium/low)
- [ ] **SQL mapping** is updated with `issue_design_mapping`
- [ ] **DESIGN.md roadmap** is updated if new item
- [ ] No **duplicate** issues exist
- [ ] **Labels** are applied (enhancement, bug, stage-X, etc.)

