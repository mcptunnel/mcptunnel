# Verification and Actions budget

- Verify changes locally before committing. Use Conventional Commits and clean up test processes.
- GitHub Actions is currently disabled to stop quota consumption. Do not enable a workflow, dispatch a run, rerun a job, or push a release tag without the user's explicit authorization for that Actions execution. A request to commit or push is not authorization to spend Actions quota.
- Before an authorized cloud run, state its platform/job scope and avoid repeating an already verified source revision. Batch implementation commits before a full candidate run.
- Once the user authorizes enabling automatic CI, ordinary source pushes to main and pull requests use Linux only; documentation-only changes do not start CI. Branch pushes must not duplicate pull-request checks.
- The full three-OS CI, release builds, official conformance and five native candidates are reserved for explicitly requested full verification or release publication. Local testing remains required while Actions is disabled.
