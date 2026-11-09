# Security Policy

## Reporting a vulnerability

Please report security vulnerabilities **privately**, through GitHub:

1. Go to the repository's **Security** tab and choose **Report a vulnerability**, or open
   <https://github.com/mohamed155/customer-service-chat-system/security/advisories/new> directly.
2. Describe the issue, the affected component or endpoint, how to reproduce it, and the impact you
   expect. A proof of concept helps, as long as it doesn't touch other people's data.

**Don't** report vulnerabilities in public issues, pull requests or discussions. The report stays
private between you and the maintainers until a fix is released.

## What to expect

| Step | Target |
| -- | -- |
| Acknowledgement of your report | Within 3 business days |
| Initial assessment (whether it is a vulnerability, and its severity) | Within 7 days |
| Fix and coordinated disclosure | Within 90 days of the report; sooner for critical issues |

We'll keep you updated while we work on it, agree a disclosure date with you, and credit you in the
published advisory unless you'd rather stay anonymous. If we can't meet a target, we'll tell you
why and when to expect an answer.

## Supported versions

The platform is developed trunk-based and released from `master`. Security fixes go to `master`
and ship with the next release; older builds are not patched separately.

## Scope

In scope: the code in this repository, which includes the backend API, the dashboard, the chat
widget, the infrastructure configuration and the operations scripts. Vulnerabilities in a
third-party dependency are in scope when they affect this project; please report those upstream
too.

When testing, only use accounts and tenants you created yourself. Don't access or change other
users' data, degrade the service for others, or use social engineering.

## Security design

How the platform is secured, including the threat model and tenant isolation, is described in
[docs/security/](docs/security/README.md).
