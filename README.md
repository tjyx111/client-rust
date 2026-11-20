# simple-vpn-client

Rust implementation of the simple VPN client.

Repository-wide development rules are defined in [AGENTS.md](AGENTS.md).

The authoritative implementation status, priorities, architecture plan, and
automated-test requirements are maintained in
[docs/project-progress.md](docs/project-progress.md).

To resume development in a new session, read
[docs/session-handoff.md](docs/session-handoff.md) before changing Rust code.

Run the local CI-equivalent S0 gate with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1
```

Unconfirmed protocol, NetFilter ABI, failure-policy, and capacity research is
kept as a future-stage draft in
[docs/design-freeze-draft.md](docs/design-freeze-draft.md).
