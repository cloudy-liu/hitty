# Issue and closed-PR audit on 2026-10-27

Queried all 151 fork PRs and every open issue in `cloudy-liu/ctty7` before
release preparation. Only issue #207 was open. No upstream issue or PR was
modified. Closed-without-merge PRs were distinguished from merged deliveries.

| Item | Verified disposition |
| --- | --- |
| [#207](https://github.com/cloudy-liu/ctty7/issues/207), Git History parent spec | Delivered by merged [#209](https://github.com/cloudy-liu/ctty7/pull/209); its implementation child #208 was already closed. Checked the spec, independent review records, native screenshots, 5,000-commit measurements, avatar request/concurrency measurements and complete source CI. Changed the PR's parent reference from `Refs` to `Closes` and explicitly closed #207 as completed, because editing a merged PR does not retroactively execute automatic closure. |
| [#188](https://github.com/cloudy-liu/ctty7/issues/188), taskbar attention | Already closed through #194. Linked the installed-group regression repair in [#212](https://github.com/cloudy-liu/ctty7/pull/212). Its accepted scope restores visible reminders through native taskbar delivery. Older vertical-taskbar, orientation-switching and Explorer-recreation checks remain unverified; this audit does not mark them passed or promise a universal upper-right position. |
| Closed unmerged [#79](https://github.com/cloudy-liu/ctty7/pull/79) | Its referenced Mermaid issue #78 is already closed; merged #80 is the implementation recorded in project status. No dangling open issue remains. |
| Closed unmerged [#60](https://github.com/cloudy-liu/ctty7/pull/60) | Its referenced Windows CI tracking issue #45 is already closed. The PR explicitly says its HTTP fixture adjustment does not resolve the older hang, so closing that PR is not counted as implementation evidence. |
| Closed unmerged [#30](https://github.com/cloudy-liu/ctty7/pull/30) | Its Markdown spec #28 is already closed. The later delivered preview/theme work is recorded in project status; this abandoned PR is not counted as an additional feature. |
| Closed unmerged [#47](https://github.com/cloudy-liu/ctty7/pull/47) and [#81](https://github.com/cloudy-liu/ctty7/pull/81) | Neither body links an issue requiring closure. The archived branding draft and its stash remain preserved; closing a PR alone is not treated as proof of delivery. |

After these actions, the fork reports zero open issues. This is a tracker
state, not a claim that every historical verification limitation has passed.
Windows-only native appearance/performance acceptance and the taskbar
configuration limits remain in the feature records and release notes.

## Taskbar code review

Both independent agents reviewed
`git diff 635eddd9364d73fe50db0ecf5c2c9c96f2504e7b...ec4046ab37375459b0ea7318b9821ab61b5ba526`.

### Standards

No actionable documented-standard, correctness or smell-baseline finding.
The review covered icon ownership, identity classification, polling before the
unchanged-artwork early return, aggregate attention, initialization failure,
and the repository's native screenshot requirement. No edits or tests were
performed by the reviewer.

### Spec

No missing accepted requirement, incorrect implementation or scope-creep
finding. The reviewer inspected all five native captures and 12 state
observations, including matched background completion, reading, minimized
selection, multiple windows and tray-disabled behavior. The older #188 shell
matrix remains a documented tracking limitation, not a passed acceptance.

Standards: 0 findings, worst issue none. Spec: 0 findings, worst issue none.
