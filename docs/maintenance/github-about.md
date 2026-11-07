# GitHub About for ctty7

This is the copy for the repository's About sidebar. GitHub stores these
fields outside Git, so merging this document does not update the sidebar.
The settings were applied after cloudy-liu/ctty7#53 was merged and the
repository was renamed on 2026-09-29.

## Description

> ctty7 is a terminal workspace for coding agents with Windows CMD/Cmder support, Herdr integration, agent status tracking, persistent sessions, WSL and SSH. Independently maintained and modified from a tty7 fork.

## Website

Use this fork's repository page instead of the upstream website, tty7.io:
`https://github.com/cloudy-liu/ctty7`.

## Apply and verify

Run with an account that can edit this fork's settings. These commands work
in PowerShell.

```powershell
$aboutRepo = 'cloudy-liu/ctty7'
$aboutDescription = 'ctty7 is a terminal workspace for coding agents with Windows CMD/Cmder support, Herdr integration, agent status tracking, persistent sessions, WSL and SSH. Independently maintained and modified from a tty7 fork.'
gh repo edit $aboutRepo --description $aboutDescription --homepage "https://github.com/$aboutRepo"
gh repo view $aboutRepo --json nameWithOwner,description,homepageUrl
```

Verify that the returned description matches the copy above and the homepage
points to this fork. Check the About sidebar on GitHub as well.
