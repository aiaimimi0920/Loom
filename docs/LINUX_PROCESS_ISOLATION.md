# Linux managed-process containment

`loom_process` requires Bubblewrap (`/usr/bin/bwrap`, distribution-maintained 0.8
or later), enabled unprivileged user/PID namespaces, and Linux pidfd support
(`pidfd_open` and `pidfd_send_signal`, Linux 5.3 or later). The executable and its
ancestors must be root-owned and not group/world-writable. Ownership checks account
for the kernel's UID translation in nested user namespaces. No PATH or plugin
environment override selects the supervisor.

## Lifetime boundary

Every managed invocation, including nested framework and MCP runtime hosts, gets
its own user/PID namespace and Bubblewrap's separate PID 1 reaper. Loom never uses
`--as-pid-1` to make an untrusted command the reaper. Before any plugin instruction
runs, a bounded `--info-fd` / `--block-fd` handshake obtains and pins namespace init
with a pidfd. The setup response is capped at 4096 bytes and a separate five-second
launch budget; a streaming caller's short protocol-request timeout is not its
startup budget. Existing one-shot execution timing still includes elapsed launch
time. A failed setup is killed and reaped before the launch
gate is dropped; EOF is not treated as safe cancellation.

On timeout, cancellation, managed-child drop, output limit, or initial command
exit, Loom sends SIGKILL through that pidfd before joining inherited pipe workers.
The kernel kills the namespace's descendants, including double-forked children,
children that call `setsid`/`setpgid` or clear `PDEATHSIG`, and nested namespaces.
This does not depend on process-group membership or `/proc` polling. The pidfd
prevents later cleanup from signalling an unrelated recycled PID. Bubblewrap's
`--die-with-parent` remains enabled as an additional lifecycle measure; the explicit
cleanup guarantee above does not depend on a plugin retaining `PDEATHSIG`.

Plugin environment is installed through Bubblewrap arguments only inside the
namespace, not in the supervisor's dynamic-loader environment. All capabilities
are dropped. A root caller maps to virtual UID/GID 65534 so a nested runtime does
not need the CAP_SETFCAP privilege required to map virtual UID 0 again. This is a
namespace identity change, not a host-filesystem ownership migration.

## Deployment and failure behavior

Install the distribution's supported Bubblewrap security updates. The Loom Docker
runtime image includes the package, but installation alone does not grant the
kernel/LSM/seccomp permissions needed to use it. A container or hardened host may
deny namespace creation, mounts, or pidfd signalling. Its administrator must provide
a narrowly reviewed host/container policy that permits this boundary. Loom does
not change global sysctls, disable an LSM, add `--privileged`, or silently bypass
isolation. Unsupported managed launches fail closed with an isolation error; other
daemon operations can remain available. CI probes the actual Bubblewrap setup and
runs the namespace regressions rather than skipping them when unsupported.

Windows keeps its suspended-start, assigned-Job-Object, verified-resume boundary.
Other operating systems have no implemented equivalent and reject managed launches
instead of using the vulnerable process-group fallback.

## Explicit limits

This is descendant lifetime containment, not a complete hostile-code sandbox. The
host filesystem and device view remain available under existing permissions; a
fresh `/proc` hides ancestor PID namespaces. Direct network, filesystem, GPU, and
clipboard capabilities keep their existing declared/audited boundaries. Memory
and active-process-count limits remain advisory on Linux, unlike Windows Job
Objects. This change does not claim containment of an already compromised host,
kernel, supervisor installation, or work delegated to an unrelated host service.

Regression tests cover escaped inherited pipes after leader exit, timeout with
blocked stdin, cancellation, managed-child drop, missing supervisor rejection,
and a nested Loom-managed runtime. Local Linux verification also runs as root and
as an ordinary user; Windows regression gates remain unchanged.
