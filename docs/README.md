# Documentation

Where the project's documentation lives. Each folder has its own index.

| Folder | What it holds |
| -- | -- |
| [`architecture/`](architecture/README.md) | How the system is built: the system architecture and the module README template |
| [`adr/`](adr/README.md) | Architecture Decision Records: one decision per file, with its context and consequences |
| [`specs/`](specs/README.md) | Where feature specifications live and how to read them |
| [`runbooks/`](runbooks/README.md) | Operating the platform: deploy, roll back, back up, restore, everyday tasks |
| [`security/`](security/README.md) | Threat model and security design notes |

Reference docs at this level:

- [Configuration reference](configuration.md): every environment variable the platform reads.

Also outside `docs/`:

- [Constitution](../.specify/memory/constitution.md): the rules every change follows.
- [CONTRIBUTING.md](../CONTRIBUTING.md): workflow, branches, commits, pull requests.
- [SECURITY.md](../SECURITY.md): how to report a vulnerability.
- `superpowers/plans/`: working implementation plans kept from earlier features.
