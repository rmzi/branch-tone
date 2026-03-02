---
description: Deploy branch-tone plugin to the marketplace
---
# /deploy — Deploy Plugin to Marketplace

Push the latest plugin manifest and marketplace files to GitHub so `claude plugin marketplace add rmzi/branch-tone` picks up changes.

## Steps

1. Validate all plugin JSON files:
   ```
   python3 -m json.tool .claude-plugin/marketplace.json
   python3 -m json.tool plugin/.claude-plugin/plugin.json
   python3 -m json.tool plugin/hooks/hooks.json
   python3 -m json.tool plugin/settings.json
   ```

2. Run tests:
   ```
   cargo test
   ```

3. Check for uncommitted plugin changes:
   ```
   git diff --name-only .claude-plugin/ plugin/
   ```
   If there are changes, commit them.

4. Push to GitHub:
   ```
   git push origin main
   ```

5. Refresh the local marketplace cache. Remove stale cache and re-add:
   ```
   claude plugin marketplace remove branch-tone
   claude plugin marketplace add rmzi/branch-tone
   ```
   Or if using a local symlink for development:
   ```
   # No action needed — symlink already points to working tree
   ls -la ~/.claude/plugins/marketplaces/branch-tone
   ```

6. Verify install works:
   ```
   claude plugin install branch-tone@branch-tone
   ```

## Local Development Shortcut

For fast iteration without pushing, symlink the marketplace cache to the local repo:
```
claude plugin marketplace remove branch-tone
ln -s /Users/rmzi/dev/personal/branch-tone ~/.claude/plugins/marketplaces/branch-tone
```
Then `claude plugin install branch-tone@branch-tone` reads directly from the working tree.

## When to Run

- After changing any file in `.claude-plugin/` or `plugin/`
- After updating hook events or permissions in the plugin manifest
- Before telling others to install the plugin
