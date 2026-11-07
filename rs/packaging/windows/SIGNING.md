# Code signing for the Windows build

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

## Policy

- Only `grimoire.exe` from the official GitHub Actions build of this repository (`.github/workflows/rust.yml`) is signed. Nothing built elsewhere is.
- Roles: [neonspellcraft](https://github.com/neonspellcraft) is the committer, reviewer and approver. Every signing request is approved by hand in SignPath.
- Privacy: Grimoire doesn't send any data over the network. It plays local files only.

## Setup (once)

1. Apply for the SignPath Foundation open-source program at https://signpath.org with this repository. Turn on two-factor sign-in for GitHub and SignPath.
2. Once approved, in the SignPath web app:
   - Create the project `grimoire`, with GitHub as its trusted build system.
   - Add the artifact configuration `windows-zip`, using the contents of [`signpath-artifact.xml`](signpath-artifact.xml).
   - Add the signing policy `release-signing`.
   - Create a CI user with submitter rights and an API token.
3. In GitHub, go to Settings → Secrets and variables → Actions:
   - Add a secret `SIGNPATH_API_TOKEN` (the token).
   - Add a variable `SIGNPATH_ORGANIZATION_ID` (from SignPath's organization settings).

## Releasing

Push a version tag (`git tag v1.0.1 && git push origin v1.0.1`), or run the workflow by hand. The Windows job submits the zip to SignPath and waits for up to an hour. Approve the request in SignPath, and the job uploads the signed zip as the `grimoire-windows-x64` artifact. Without the secret and variable, builds stay unsigned.
