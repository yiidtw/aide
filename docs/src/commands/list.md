# aide list

List all registered agents.

## Usage

```bash
aide list
```

## Output

```
NAME                 PATH                                               STATUS
────────────────────────────────────────────────────────────────────────────────
reviewer             ~/projects/code-reviewer                           issue
writer               ~/projects/blog-writer                             manual
ops                  ~/.aide/ops                                        cron:0 9 * * *
```

The STATUS column shows the trigger type from each agent's Aidefile. Shows `"missing"` if the Aidefile can't be found, or `"error"` if it can't be parsed.
