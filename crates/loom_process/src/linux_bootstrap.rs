//! Kernel-carried supervisor descriptors cross the managed PID-namespace boundary.

use std::io;
use std::mem::{size_of, zeroed};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixDatagram;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub(crate) const ENV_KEY: &str = "LOOM_PROCESS_SUPERVISOR_FD";
const MAGIC: &[u8; 8] = b"LOOMSUP1";

pub(crate) fn prepare(command: &mut Command, files: [RawFd; 2]) -> io::Result<UnixDatagram> {
    let (sender, receiver) = UnixDatagram::pair()?;
    let mut control = [0usize; 8];
    let mut iov = libc::iovec {
        iov_base: MAGIC.as_ptr().cast_mut().cast(),
        iov_len: MAGIC.len(),
    };
    let mut message: libc::msghdr = unsafe { zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = unsafe { libc::CMSG_SPACE(size_of::<[RawFd; 2]>() as u32) } as usize;
    unsafe {
        let header = libc::CMSG_FIRSTHDR(&message);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(size_of::<[RawFd; 2]>() as u32) as usize;
        std::ptr::copy_nonoverlapping(
            files.as_ptr().cast::<u8>(),
            libc::CMSG_DATA(header),
            size_of::<[RawFd; 2]>(),
        );
        if libc::sendmsg(
            sender.as_raw_fd(),
            &message,
            libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
        ) != MAGIC.len() as isize
        {
            return Err(io::Error::last_os_error());
        }
    }
    let fd = receiver.as_raw_fd();
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(receiver)
}

pub(crate) fn receive(fd: RawFd) -> io::Result<[OwnedFd; 2]> {
    if fd <= 2 {
        return Err(io::Error::other("invalid supervisor channel descriptor"));
    }
    let mut peer: libc::ucred = unsafe { zeroed() };
    let mut size = size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut peer as *mut libc::ucred).cast(),
            &mut size,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    // A socketpair created by our launcher is outside this managed PID namespace.
    // Same-namespace code cannot spoof that with an environment variable and a
    // locally created socketpair. This is ancestor launch authority, not an
    // authentication mechanism for a malicious caller supervising itself.
    if peer.pid != 0
        || peer.uid == u32::MAX
        || peer.gid == u32::MAX
        || size as usize != size_of::<libc::ucred>()
    {
        return Err(io::Error::other(
            "supervisor channel must originate in an ancestor PID namespace",
        ));
    }
    let channel = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut payload = [0; 9];
    let mut control = [0usize; 8];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };
    let mut message: libc::msghdr = unsafe { zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = size_of_val(&control);
    let read = unsafe {
        libc::recvmsg(
            channel.as_raw_fd(),
            &mut message,
            libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC,
        )
    };
    if read < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut files = Vec::new();
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let count = ((*header).cmsg_len - libc::CMSG_LEN(0) as usize) / size_of::<RawFd>();
                for index in 0..count {
                    files.push(OwnedFd::from_raw_fd(std::ptr::read_unaligned(
                        libc::CMSG_DATA(header).cast::<RawFd>().add(index),
                    )));
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    // Even malformed/truncated messages adopt every delivered descriptor first,
    // so rejecting a forged frame cannot leak kernel-installed SCM_RIGHTS handles.
    if read != MAGIC.len() as isize
        || &payload[..8] != MAGIC
        || message.msg_flags & (libc::MSG_CTRUNC | libc::MSG_TRUNC) != 0
        || files.len() != 2
    {
        return Err(io::Error::other("invalid supervisor descriptor frame"));
    }
    files
        .try_into()
        .map_err(|_| io::Error::other("invalid supervisor descriptor count"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_namespace_cannot_forge_ancestor_supervisor_channel() {
        let (_sender, receiver) = UnixDatagram::pair().unwrap();
        let error = receive(receiver.as_raw_fd()).unwrap_err();
        assert!(error.to_string().contains("ancestor PID namespace"));
    }

    #[test]
    fn unconnected_socket_is_not_an_ancestor_channel() {
        let receiver = UnixDatagram::unbound().unwrap();
        let error = receive(receiver.as_raw_fd()).unwrap_err();
        assert!(error.to_string().contains("ancestor PID namespace"));
    }
}
