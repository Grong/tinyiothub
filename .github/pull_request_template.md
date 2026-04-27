## Summary

<!-- Brief description of what this PR does and why -->

## Changes

- 

## Test plan

- [ ] `cargo check --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] Frontend: `pnpm type-check && pnpm build` passes (if applicable)

## Related issues

<!-- Link any related issues: Fixes #123, Relates to #456 -->

## Checklist

- [ ] I have read `CLAUDE.md` and my changes comply with architecture rules
- [ ] New files follow the `types → service → handler` module structure
- [ ] API responses use `ApiResponseBuilder`
- [ ] Database access goes through Repository pattern
- [ ] No secrets or credentials in the diff
