# Snapshots of the data

Before every update, `xctl update` takes a snapshot of the data partition:
an instant, read-only copy (Btrfs) that takes no space until files change.
The newest 3 are kept in `/.snapshots`. The system itself (`/usr`) does not
need them: it has its own previous version ([rollback.md](rollback.md)).

## Commands

    xctl snapshots                          # the list, oldest first
    sudo xctl snapshots create              # one more, now
    sudo xctl snapshots restore <name>      # Recovery only

Names are `<date>_<time>-<reason>`, for example
`2026-11-12_101500-before-26.11-testing.60`.

## Restoring

Restoring brings back the settings (`/etc`) and the users' files (`/home`)
as they were in the snapshot. It works only in Recovery, where nobody is
logged in: **Restore data** in the Recovery menu, or
`xctl snapshots restore <name>` from its shell. The current state is first
saved as one more snapshot (`...-before-restore`), so a restore can be
undone too. `/var` (logs, caches, the system's own state) is left as it is.

## Tests

`tools/ci/snapshots.sh`: a file written before a snapshot comes back after
the restore, a file added after it is gone, the user still logs in, and the
state before the restore is kept as a snapshot. `tools/ci/update.sh` checks
the snapshot taken by an update.
