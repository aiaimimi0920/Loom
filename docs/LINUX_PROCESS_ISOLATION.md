# Linux managed-process containment

`loom_process` requires Bubblewrap (`/usr/bin/bwrap`, distribution-maintained 0.8
or later), distribution `perl-base` (`/usr/bin/perl`, for an exec-only acknowledgement
shim), enabled unprivileged user/PID namespaces, and Linux pidfd support
(`pidfd_open` and `pidfd_send_signal`, Linux 5.3 or later). The executable and its
ancestors must be root-owned and not group/world-writable at the outer launch
boundary, where the initial user namespace identity map makes ownership
distinguishable. Neither a nested virtual UID 0 nor `overflowuid` proves host root
ownership. No PATH or plugin environment override selects the supervisor.

The outer launcher opens and pins the validated Bubblewrap and Perl files. Each
managed child receives those kernel file references in a bounded, one-use
`SCM_RIGHTS` frame over an inherited Unix socketpair. The receiver requires valid
`SO_PEERCRED` credentials originating outside its PID namespace; same-namespace
and unconnected forged sockets are rejected. `LOOM_PROCESS_SUPERVISOR_FD` is an
internal descriptor locator, never a package-selected configuration value.
Explicit plugin environment cannot replace it. Nested launchers cache the
received files and execute through `/proc/self/fd`, so bind-mounting a fake file
over `/usr/bin/bwrap` or `/usr/bin/perl` cannot retarget them. Each nested managed
launch forwards a new one-use frame, without a broker thread or unbounded queue.

A standalone caller already in a remapped user namespace without this inherited
launch capability fails closed, including rootless containers that have no trusted
outer bootstrap. Ordinary nonroot users in the initial namespace and their managed
nested runtimes are supported. Unmanaged intermediaries must not strip the launch
descriptor when they intend to start a nested managed runtime. This protocol
preserves ancestor launch authority; it does not authenticate a malicious program
supervising itself, nor grant it authority to escape the ancestor PID boundary.

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

After namespace admission, a private close-on-exec pipe acknowledges the actual
target exec. Missing/non-executable targets remain `ProcessError::Spawn`, not an
ordinary exit code; target exit 127 is not confused with launch failure. The shim
runs with a scrubbed environment and installs plugin environment only immediately
before exec, never in Bubblewrap's or the shim's loader environment. The shim's
fixed source and arguments are passed separately, without shell interpolation.
An expired one-shot deadline or cancellation leaves the launch gate closed and
tears down the namespace. Streaming request timeouts remain separate from setup.
All capabilities
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

Ubuntu 24.04 CI explicitly loads an ephemeral AppArmor profile attached only to
`/usr/bin/bwrap`, granting `userns` while leaving the global restriction enabled.
This follows Ubuntu's [application-specific user namespace policy](https://documentation.ubuntu.com/release-notes/24.04/#unprivileged-user-namespace-restrictions).
The allowance is inherited inside the namespace so nested runtimes work; it is
not a new general-purpose sandbox policy. Loom does not install it on user hosts.
Deployment administrators must review their own equivalent policy rather than
turn off AppArmor or suppress a failed prerequisite probe.

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
