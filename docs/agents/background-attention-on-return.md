# Background Agent attention on return

The taskbar and tray mark calls the user back to ctty7 when a live Agent turn
finishes or requests input while the entire application is in the background.
It is independent of the sound and desktop-notification settings.

Activating any ctty7 window immediately clears the application mark, including
when a different tab is selected. Tab and pane states remain independent: an
unviewed result stays unread and a waiting Agent stays in the input state.
Switching away again does not revive an acknowledged mark; a later background
event can create a new one. A foreground event never creates a background mark.

The sound settings retain their existing Never / Unfocused / Always behavior.
Initial or reconnected Agent snapshots are not new events. Multiple windows
share the same application-level attention state.
