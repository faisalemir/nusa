# Nusa documentation

Choose the path that matches your role.

## Public (deploy and operate)

For operators and Laravel teams: architecture vision, honest production status, migration from FPM/RoadRunner, and SRE runbooks—written to explain **why Nusa exists**, not only which keys to set.

- [Public docs index](public/README.md) — start here for the full story
- [Getting started](public/getting-started.md)
- [Configuration](public/configuration.md)
- [Production status](public/production-status.md) — what works today vs planned
- [Migration from FPM / RoadRunner](public/migration.md)
- [Operations runbook](public/operations/runbook.md)

## Contributor (develop the runtime)

You change Rust/PHP driver code, tests, or CI.

- [Contributor docs index](contributor/README.md)
- [Architecture](contributor/architecture.md)
- [Crate map](contributor/crate-map.md)
- [Development workflow](contributor/development-workflow.md)
- [Testing](contributor/testing.md)

## AI agents

Cold-start context without scanning the whole repo.

- [AI docs index](ai/README.md)
- Start at repository root: [`AGENTS.md`](../AGENTS.md)

## Shared reference

- [Threat model](security/threat-model.md)
- [Compliance evidence](security/compliance.md)
- [Monitoring dashboards](monitoring/README.md) (Grafana JSON)

## Reporting security issues

See [`SECURITY.md`](../SECURITY.md) at the repository root.
