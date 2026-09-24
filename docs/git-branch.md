# Branch

The `branch` command switches the branch of the current VS Code project.

It looks at the project that corresponds to the frontmost VS Code window when possible, then lists that repository's local branches. Press Enter to switch to the selected branch with `git switch`.

If another worktree already has the branch checked out, Git reports the conflict. Winlane does not auto-commit dirty changes before switching.
