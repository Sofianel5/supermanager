## Supermanager

You are connected to a coordination server that gives your manager real-time visibility into what their team is working on. Other developers on the team are also connected — their coding agents report progress too.

### Rules

- You MUST call `submit_progress` as your FIRST action in every conversation, before doing any other work. Report that you are starting and what the user asked for.
- Call `submit_progress` again whenever you: make meaningful progress, change approach, encounter a blocker, or finish the task.
- When in doubt, over-report. Your manager would rather have too many updates than too few.
- Include `submit_progress` in parallel with other tool calls — never delay your work to report.
- Before the first `submit_progress` call in a repository, resolve `employee_name`, `repo`, and `branch` from the local environment if they are not already known.
- Never send blank strings or placeholder values such as `"unknown"` for `employee_name`, `repo`, or `branch`. Resolve the real values first.

### Field guidance

- `employee_name`: The name of the human user you are assisting. Never use "Claude" or your own name. First try `git config user.name`. If that returns empty, try `whoami`. If BOTH are empty, tell the user: "Please set your name with: git config --global user.name 'Your Name'" and do NOT call submit_progress with empty or placeholder values.
- `repo`: The git remote URL of the repository you are working in. Use `git remote get-url origin` if needed.
- `branch`: The current git branch. Use `git branch --show-current` if needed.
- `progress_text`: A concise, informative summary written for a manager audience. Focus on what was done and why, not implementation details. Examples:
  - "Starting work on adding pagination to the /users endpoint per user request."
  - "Refactored the database query layer — replaced raw SQL with parameterized queries across 4 files."
  - "Blocked: test suite fails due to missing fixture data, investigating."
  - "Finished: implemented and tested the new auth middleware. All tests pass."

### Recommended startup lookup

When preparing the first `submit_progress` call, gather the required fields immediately with:

```sh
git config user.name || whoami
git branch --show-current
git remote get-url origin
```

Do not call `submit_progress` until those commands have been checked when the values are not already known from context.
