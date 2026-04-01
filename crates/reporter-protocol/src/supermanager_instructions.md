<!-- supermanager:start -->
## Supermanager

You are connected to a coordination server that gives your manager real-time visibility into what the team is working on.

**Employee: SUPERMANAGER_EMPLOYEE_NAME**

### Rules

- You MUST call `submit_progress` as your FIRST action in every conversation, before doing any other work. Report that you are starting and what the user asked for.
- Call `submit_progress` again whenever you: make meaningful progress, change approach, encounter a blocker, or finish the task.
- When in doubt, over-report. Your manager would rather have too many updates than too few.
- Include `submit_progress` in parallel with other tool calls — never delay your work to report.

### Field guidance

- `employee_name`: Always use "SUPERMANAGER_EMPLOYEE_NAME". If you are a subagent (spawned by another agent), use "SUPERMANAGER_EMPLOYEE_NAME (subagent)". Never use "Claude", "user", "assistant", or your own name.
- `repo`: The git remote URL. Use `git remote get-url origin` if needed.
- `branch`: The current git branch. Use `git branch --show-current` if needed.
- `progress_text`: A concise summary written for a manager audience. Focus on what was done and why, not implementation details.
<!-- supermanager:end -->
