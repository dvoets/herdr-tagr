# Platforms

| Platform | Transport | Status |
|---|---|---|
| Linux | Unix domain socket | Developed and used here; verified against a live herdr session |
| WSL | Unix domain socket | Same build and transport as Linux |
| macOS | Unix domain socket | Same code path as Linux; built and tested in CI |
| Windows | Named pipe | Built and tested in CI; **not exercised against a live herdr session** |

Be aware of that last row. Windows compiles, passes the test suite and passes
clippy in CI on every push, and the named-pipe transport is the documented way
plugins reach herdr there - but nobody has yet run it against herdr on Windows.
Bug reports welcome.

## How the endpoint is found

herdr injects `HERDR_SOCKET_PATH` into every plugin command, and that is used
as-is on all platforms. Order of resolution:

1. `socket_path` in the plugin config, if set.
2. `HERDR_SOCKET_PATH`.
3. The platform default: `~/.config/herdr/herdr.sock` on Unix,
   `\\.\pipe\herdr` on Windows.

Only step 3 matters when running the binary by hand outside herdr.

## Platform-specific behaviour

**Read and write timeouts** bound a request on Unix. The standard library
exposes no timeout for a named pipe handle, so on Windows a request blocks
until herdr answers.

**Existence checks.** A missing socket file is reported as a clear error on
Unix. `\\.\pipe\...` does not answer existence checks the same way, so on
Windows the connect attempt is the test.

**The home directory** is `HOME`, falling back to `USERPROFILE`.

**The binary name.** Cargo emits `herdr-tagr.exe` on Windows, but a plugin
command is one string for every platform, and herdr skips PATHEXT resolution
for a path containing separators. A Windows-only build step copies the binary
to an extensionless name so the single declared path is valid everywhere.

## Nothing else is platform-specific

Process detection goes through `pane.process_info` rather than `/proc`, so it
behaves the same everywhere. Paths use `Path`/`PathBuf` throughout, and
identifying a process by basename accepts both separators.

## WSL

WSL is Linux: build and run it as Linux, alongside a herdr running inside the
same WSL distribution.

Running the plugin inside WSL against a herdr running on the Windows host is
not supported - that would need to cross from a Linux process to a Windows
named pipe.
