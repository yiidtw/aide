# Cron & Scheduling

Run skills on a schedule using cron expressions.

## Defining schedules in Agentfile.toml

Add a `schedule` field to any skill:

```toml
[skills.cool]
script = "skills/cool.sh"
description = "NTU COOL LMS scanning"
schedule = "0 8 * * *"           # daily at 8:00 AM

[skills.email]
script = "skills/email.sh"
description = "Email triage"
schedule = "0 */4 * * *"         # every 4 hours
```

The schedule uses standard 5-field cron syntax:

```
minute  hour  day-of-month  month  day-of-week
  0      8        *           *        *
```

## Managing schedules at runtime

### List scheduled jobs

```bash
$ aide.sh cron ls
INSTANCE  SKILL   SCHEDULE       NEXT RUN
jenny     cool    0 8 * * *      2025-06-15 08:00
jenny     email   0 */4 * * *    2025-06-15 12:00
```

### Add a schedule

```bash
$ aide.sh cron add jenny cool "30 9 * * 1-5"
Schedule set: jenny/cool at 30 9 * * 1-5 (weekdays at 9:30 AM)
```

### Remove a schedule

```bash
$ aide.sh cron rm jenny cool
Schedule removed: jenny/cool
```

## Daemon mode

Scheduled jobs only run when the daemon is active:

```bash
$ aide.sh up
Daemon started (PID 12345)
Dashboard: http://localhost:3939
Cron scheduler: active (2 jobs)
```

Without the daemon, schedules defined in Agentfile.toml are stored but not executed.

To stop the daemon:

```bash
$ aide.sh down
Daemon stopped.
```

## Viewing cron status in the dashboard

The dashboard at `http://localhost:3939` shows a cron panel with:

- All scheduled jobs across all instances
- Next scheduled run time
- Last execution result (exit code, duration, truncated output)
- Execution history (last 10 runs per job)

## Cron output and logs

Cron job output is captured in the instance log:

```bash
$ aide.sh logs jenny --filter cron
[2025-06-14 08:00:01] cron/cool: 3 courses, 2 new assignments
[2025-06-14 12:00:01] cron/email: 5 unread messages
```

Failed jobs (non-zero exit code) are flagged in the dashboard and logs.
