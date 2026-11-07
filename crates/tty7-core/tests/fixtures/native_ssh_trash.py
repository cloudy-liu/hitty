"""Disposable loopback SSH/SFTP fixture; requires asyncssh in the test venv."""
import asyncio
import json
import os
import pathlib
import tempfile
import asyncssh


async def main():
    root = pathlib.Path(tempfile.mkdtemp(prefix="ctty7-native-trash-"))
    home = root / "home"
    home.mkdir(mode=0o700)
    xdg = home / "custom-data"
    xdg.mkdir(mode=0o700)
    key = asyncssh.generate_private_key("ssh-ed25519")
    key_path = root / "client-key"
    key.write_private_key(str(key_path))
    os.chmod(key_path, 0o600)

    class Server(asyncssh.SSHServer):
        def begin_auth(self, username): return True
        def public_key_auth_supported(self): return True
        def validate_public_key(self, username, offered):
            return username == "fixture" and offered.export_public_key() == key.export_public_key()

    async def command(process):
        # Only the trusted test binary sends a fixed command. File paths are
        # passed separately on stdin; HOME/XDG never point at the user's data.
        data = await process.stdin.read()
        child = await asyncio.create_subprocess_shell(
            process.command, stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
            cwd=str(home), env={**os.environ, "HOME": str(home), "XDG_DATA_HOME": str(xdg)},
        )
        stdout, stderr = await child.communicate(data.encode())
        process.stdout.write(stdout.decode())
        process.stderr.write(stderr.decode())
        process.exit(child.returncode)

    server = await asyncssh.create_server(
        Server, "127.0.0.1", 0,
        server_host_keys=[asyncssh.generate_private_key("ssh-ed25519")],
        process_factory=command,
        sftp_factory=asyncssh.SFTPServer,
    )
    print(json.dumps({"port": server.get_port(), "key": str(key_path), "home": str(home), "xdg": str(xdg)}), flush=True)
    try:
        await asyncio.Event().wait()
    finally:
        server.close()
        await server.wait_closed()


asyncio.run(main())
