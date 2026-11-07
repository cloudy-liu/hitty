# 从旧 fork 迁移到 ctty7

ctty7 v0.1.0 基于本 fork，保留原有配置、会话目录、命令和安装身份。
最新旧版为 v26.8.3-c.9。第一次迁移需要手动安装：旧更新器按版本大小比较，
不会把 0.1.0 识别为升级。仓库改名也可能让旧版手动检查更新显示失败。

## 安装步骤

1. 从 [正式 Release](https://github.com/cloudy-liu/ctty7/releases/latest) 下载对应平台的包，
   根据 `checksums.txt` 核对 SHA-256。迁移前保存正在运行的工作，并备份原配置目录。
2. 退出应用并停止后台服务。停止服务会结束面板内的 shell、Agent 和 SSH 进程；
   请在任务完成后操作，不能把布局恢复当成进程保活。
3. Windows 安装版运行新的 Setup，保持原安装范围，复用原目录。不要先卸载。
   新安装默认使用 ctty7 目录；旧安装目录即使仍叫 tty7 也正常。
4. Windows 便携版和 Linux 压缩包将完整程序包解压到原位置，替换程序文件。
   macOS 用新包中的 `tty7.app` 替换原应用；AppImage 替换原文件。
   不要删除配置目录，也不要同时运行两个共享配置的版本。
5. 启动 ctty7，检查原工作区、分屏、shell 配置及 Agent 会话。已知会话 ID 的受支持
   Agent 可以恢复对话，无法确定会话时保留可用 shell。
6. 如果任务栏固定图标仍显示旧名称或图标，取消固定，再从新的开始菜单入口固定。

配置仍位于 Windows 的 `%APPDATA%\tty7` 或 macOS/Linux 的 `~/.config/tty7`。
`TTY7_CONFIG_DIR` 和 `--config-dir` 的原有覆盖方式继续有效。命令仍为 `tty7`。
原配置中的 `update_channel` 会被忽略，并在下次保存时移除；其他配置保留。
旧版暂存、尚未安装的更新计划会作废，避免迁移后装回旧发行线。

## 后续更新

ctty7 只有正式 Release 渠道。0.1.0 之后按 0.1.1、0.1.2 等版本升级，
在设置的关于页面检查更新。安装包会先下载并校验，应用更新需要明确操作。
内部 `tty7-server-*` 名称和通信标识保持兼容。

## English migration steps

Install ctty7 v0.1.0 manually once from the official Release. The old 26.x
updater considers it a downgrade; repository renaming can also break the old
manual update check. Save running work, back up configuration, then quit the
application and stop its background server. Stopping the server ends live
shells, agents and SSH processes.

For Windows Setup installs, keep the previous per-user/all-users scope and
installation directory; do not uninstall first. For portable Windows/Linux
archives, replace the complete program payload. On macOS replace the existing
`tty7.app`; for AppImage replace the old file. Keep configuration and session
data. Re-pin the Start menu shortcut if Windows retains an old taskbar icon.

Configuration remains in `%APPDATA%\tty7` or `~/.config/tty7`; existing directory
overrides still work. The command remains `tty7`. Legacy `update_channel`
settings are ignored and removed on save. Pre-migration staged updates are invalidated. From 0.1.0 onward, official releases
update normally. Layout and supported agent-conversation restoration do not
preserve running processes across a server stop or system reboot.
