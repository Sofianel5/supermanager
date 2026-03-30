---
name: reporting
description: Inspect what the progress reporter is sending in realtime and help validate the coordination server configuration.
---

# Reporting

Use this skill when the user wants to:

- inspect what the progress reporter is sending to the coordination server
- explain the `intent` and `progress` note flow
- validate which coordination server URL is configured

## Guidance

- Prefer showing the exact text being submitted by the hook wrapper or `reporter-cli`.
- The current MVP sends freeform progress notes directly; it does not buffer or batch them locally.
