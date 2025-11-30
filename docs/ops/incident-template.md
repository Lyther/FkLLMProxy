# Incident Post-Mortem: [Incident Title]

**Date**: YYYY-MM-DD
**Authors**: [Names]
**Status**: [Draft/Review/Published]
**Impact**: [Severity Level, e.g., SEV-1]
**Root Cause**: [Brief Summary]

## 1. Executive Summary

*Briefly describe the incident, its impact on users, and the resolution.*

## 2. Impact

- **Time to Detect**: [Duration]
- **Time to Mitigation**: [Duration]
- **Time to Resolution**: [Duration]
- **Users Affected**: [Number/%]
- **Data Loss**: [Yes/No]

## 3. Timeline

*All times in UTC.*

- **[HH:MM]**: Alert triggered / Issue reported by [Source].
- **[HH:MM]**: Incident declared. Commanded by [Name].
- **[HH:MM]**: Mitigation applied [Action]. Impact stabilized.
- **[HH:MM]**: Root cause identified.
- **[HH:MM]**: Fix deployed.
- **[HH:MM]**: Incident closed.

## 4. Root Cause Analysis (The 5 Whys)

1. **Why did the system fail?**
   - [Answer]
2. **Why?**
   - [Answer]
3. **Why?**
   - [Answer]
4. **Why?**
   - [Answer]
5. **Why?**
   - [Answer]

## 5. Mitigation & Resolution

*How did we stop the bleeding? How did we fix it permanently?*

## 6. Lessons Learned

### What went well?

- [Item 1]
- [Item 2]

### What went wrong?

- [Item 1]
- [Item 2]

### Where we got lucky?

- [Item 1]

## 7. Action Items

| Task | Type | Owner | Priority | Status |
|------|------|-------|----------|--------|
| Add circuit breaker to X | Prevent | @dev | P0 | Open |
| Update runbook for Y | Process | @sre | P1 | Open |
