# Capability runtime templates

These templates implement `loom.capability.runtime.v1` without importing Loom
or Hook source. Every message is UTF-8 JSON inside a four-byte unsigned
big-endian length frame, with a hard 4 MiB payload limit.

The examples handle `initialize`, `activate`, `command`, and `deactivate`.
Replace the command body with your capability and keep lifecycle responses
bounded. Protocol logs belong on stderr; stdout is reserved for frames.

## Run all templates

Install Rust, Node.js 22 or newer, Python 3.10 or newer, and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Test-Templates.ps1
```

`fake_host.py` starts one runtime, checks request correlation and
protocol fields, exercises the four lifecycle methods, verifies a command
result, and requires a clean exit after deactivation.

## Package a runtime

Build or bundle the selected language runtime as a Windows executable. Use:

```powershell
.\loom-plugin.exe init capability .\my-capability my-capability publisher.example
```

Replace the generated runtime placeholder, then update the signed manifest's
commands, schemas, permissions, entry command, resource limits, and publisher
key. Validate, sign, and pack with the commands in
`docs/plugin-development.md`. Interpreters may be packaged as the service
entry with a package-relative script in `args`; do not depend on host PATH in
a distributed capability.
