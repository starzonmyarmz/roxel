# Security policy

## Supported versions

Only the latest release gets security fixes. Roxel has no long-term support branches; update to the newest version from the [Releases page](https://github.com/starzonmyarmz/roxel/releases).

## Reporting a vulnerability

Please don't open a public issue for a security problem. Report it privately through GitHub instead:

1. Go to the repository's [Security tab](https://github.com/starzonmyarmz/roxel/security).
2. Click **Report a vulnerability**.
3. Describe the problem, the Roxel version and OS, and how to reproduce it. A sample file helps if the problem is in a file importer.

You should hear back within a week. Roxel is maintained by one person in their spare time, so a fix can take longer. You'll be credited in the advisory unless you'd rather not be.

## Scope

Things worth reporting include:

- a crafted `.rox`, `.vox`, `.qb`, `.gox` or `.ase` file that crashes Roxel, hangs it or makes it write outside the place you chose
- anything that would let a release or the update check be tampered with

Roxel makes one network request: the optional update check to GitHub's releases API. It never downloads or runs code from the network. See [Preferences → Updates](https://starzonmyarmz.github.io/roxel/preferences.html#updates).
