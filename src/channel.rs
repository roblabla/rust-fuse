//!
//! Raw communication channel to the FUSE kernel driver.
//!

use std::io;
use std::ffi::{CString, CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::{PathBuf, Path};
use libc::{self, c_int, c_void, size_t};
use fuse::fuse_args;
use reply::ReplySender;

/// Helper function to provide options as a fuse_args struct
/// (which contains an argc count and an argv pointer)

fn with_fuse_args<T, F: FnOnce(&fuse_args) -> T> (options: &[&OsStr], f: F) -> T {
    let mut args = vec![CString::new("rust-fuse").unwrap()];
    args.extend(options.iter().map(|s| CString::new(s.as_bytes()).unwrap()));
    let argptrs: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
    f(&fuse_args { argc: argptrs.len() as i32, argv: argptrs.as_ptr(), allocated: 0 })
}

/// A raw communication channel to the FUSE kernel driver
#[derive(Debug)]
pub struct Channel {
    mountpoint: PathBuf,
    fd: c_int,
}

use libc::getuid;
use libc::getgid;
use libc::mount;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::IntoRawFd;

fn fuse_mount_sys(mountpoint: &PathBuf, flags: u64) -> i32
{
    // TODO:Check args
    // TODO:Check mountpoint
    // TODO:Check nonempty
    // TODO:Check auto_umount
    let f = OpenOptions::new().read(true).write(true).open("/dev/fuse").unwrap();


    // TODO:Check f
    // from:sdcard.c    sprintf(opts, "fd=%i,rootmode=40000,default_permissions,allow_other,"
    //                                "user_id=%d,group_id=%d", fd, uid, gid);
    let opts = format!("fd={},rootmode={},default_permissions,allow_other,user_id={},group_id={}",
                       f.as_raw_fd(),40000,
                       unsafe{getuid()}, unsafe{getgid()});
    // TODO: Add kernel opt
    // 
    //TODO: understand:
    /*
	strcpy(type, mo->blkdev ? "fuseblk" : "fuse");
	if (mo->subtype) {
		strcat(type, ".");
		strcat(type, mo->subtype);
	}
	strcpy(source,
	       mo->fsname ? mo->fsname : (mo->subtype ? mo->subtype : devname));
    */
    info!("{}", opts);
    let c_sources = CString::new("/dev/fuse").unwrap();
    let c_mnt = CString::new(mountpoint.as_os_str().as_bytes()).unwrap();
    let c_fs = CString::new("fuse").unwrap();
    let c_opts = CString::new(opts).unwrap();
    info!("MOUNT({:?} {:?} {:?} {:?})", c_sources, c_mnt, c_fs, c_opts);
    let res = unsafe{
        #[cfg(target_pointer_width = "32")]
        let flags = flags as u32;
        mount(c_sources.as_ptr(), c_mnt.as_ptr(), c_fs.as_ptr(), flags, c_opts.as_ptr() as *mut c_void)
    };
    if res < 0 {
        return res;
    } else {
        return f.into_raw_fd();
    }
}

// /usr/include/sys/mount.h
const MS_NOSUID :u64 = 2;
const MS_NODEV  :u64 = 4;

fn fuse_mount_fusermount(mountpoint: &PathBuf, _: &fuse_args) -> i32
{
    return 0 // TODO:
}

fn fuse_kern_mount(mountpoint: &PathBuf, args: &fuse_args) -> i32
{
    let flags = MS_NOSUID | MS_NODEV;

    // TODO: parse fuse_args
    /*
       pub argc: c_int,
       pub argv: *const *const c_char,
       pub allocated: c_int,
       */
    // TODO: check if allow_other and allow_root aren't mutually active
    // TODO: check if help
    // TODO: get kernel/other flags options

    let res = fuse_mount_sys(mountpoint, flags);
    if res < 0 {
        // TODO: error
        if res == libc::EPERM {
            warn!("fuse_mount_sys no enougth permission for mount_sys");
            return fuse_mount_fusermount(mountpoint, args);
        } else {
            error!("fuse_mount_sys unknown ERROR: {}", res);
            panic!("fuse_mount_sys panic!");
        }
    }
    println!("fantafs: fuse_mount_compat25: fd={}", res);
    // TODO: ERROR
    res
}

fn fuse_mount_compat25(mountpoint: &PathBuf, args: &fuse_args) -> i32
{
    fuse_kern_mount(mountpoint, args)
}

impl Channel {
    /// Create a new communication channel to the kernel driver by mounting the
    /// given path. The kernel driver will delegate filesystem operations of
    /// the given path to the channel. If the channel is dropped, the path is
    /// unmounted.
    pub fn new (mountpoint: &Path, options: &[&OsStr]) -> io::Result<Channel> {
        let mountpoint = try!(mountpoint.canonicalize());
        with_fuse_args(options, |args| {
            let fd = fuse_mount_compat25(&mountpoint, args);
            if fd < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(Channel { mountpoint: mountpoint, fd: fd })
            }
        })
    }

    /// Return path of the mounted filesystem
    pub fn mountpoint (&self) -> &Path {
        &self.mountpoint
    }

    /// Receives data up to the capacity of the given buffer (can block).
    pub fn receive (&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        let rc = unsafe { libc::read(self.fd, buffer.as_ptr() as *mut c_void, buffer.capacity() as size_t) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            unsafe { buffer.set_len(rc as usize); }
            Ok(())
        }
    }

    /// Returns a sender object for this channel. The sender object can be
    /// used to send to the channel. Multiple sender objects can be used
    /// and they can safely be sent to other threads.
    pub fn sender (&self) -> ChannelSender {
        // Since write/writev syscalls are threadsafe, we can simply create
        // a sender by using the same fd and use it in other threads. Only
        // the channel closes the fd when dropped. If any sender is used after
        // dropping the channel, it'll return an EBADF error.
        ChannelSender { fd: self.fd }
    }
}

impl Drop for Channel {
    fn drop (&mut self) {
        // TODO: send ioctl FUSEDEVIOCSETDAEMONDEAD on OS X before closing the fd
        // Close the communication channel to the kernel driver
        // (closing it before unnmount prevents sync unmount deadlock)
        unsafe { libc::close(self.fd); }
        // Unmount this channel's mount point
        let _ = unmount(&self.mountpoint);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChannelSender {
    fd: c_int,
}

impl ChannelSender {
    /// Send all data in the slice of slice of bytes in a single write (can block).
    pub fn send (&self, buffer: &[&[u8]]) -> io::Result<()> {
        let iovecs: Vec<_> = buffer.iter().map(|d| {
            libc::iovec { iov_base: d.as_ptr() as *mut c_void, iov_len: d.len() as size_t }
        }).collect();
        let rc = unsafe { libc::writev(self.fd, iovecs.as_ptr(), iovecs.len() as c_int) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl ReplySender for ChannelSender {
    fn send(&self, data: &[&[u8]]) {
        if let Err(err) = ChannelSender::send(self, data) {
            error!("Failed to send FUSE reply: {}", err);
        }
    }
}

/// Unmount an arbitrary mount point
pub fn unmount (mountpoint: &Path) -> io::Result<()> {
    // fuse_unmount_compat22 unfortunately doesn't return a status. Additionally,
    // it attempts to call realpath, which in turn calls into the filesystem. So
    // if the filesystem returns an error, the unmount does not take place, with
    // no indication of the error available to the caller. So we call unmount
    // directly, which is what osxfuse does anyway, since we already converted
    // to the real path when we first mounted.

    #[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "dragonfly",
              target_os = "openbsd", target_os = "bitrig", target_os = "netbsd"))] #[inline]
    fn libc_umount (mnt: &CStr) -> c_int { unsafe { libc::unmount(mnt.as_ptr(), 0) } }

    #[cfg(not(any(target_os = "macos", target_os = "freebsd", target_os = "dragonfly",
                  target_os = "openbsd", target_os = "bitrig", target_os = "netbsd")))] #[inline]
    fn libc_umount (mnt: &CStr) -> c_int {

        // TODO: Recode fuse_unmount_compat22 in pure rust.
        // This impl might not work if the process calling umount is not root.
        let rc = unsafe { libc::umount(mnt.as_ptr()) };
        rc
    }

    let mnt = try!(CString::new(mountpoint.as_os_str().as_bytes()));
    let rc = libc_umount(&mnt);
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}


#[cfg(test)]
mod test {
    use super::with_fuse_args;
    use std::ffi::{CStr, OsStr};

    #[test]
    fn fuse_args () {
        with_fuse_args(&[OsStr::new("foo"), OsStr::new("bar")], |args| {
            assert_eq!(args.argc, 3);
            assert_eq!(unsafe { CStr::from_ptr(*args.argv.offset(0)).to_bytes() }, b"rust-fuse");
            assert_eq!(unsafe { CStr::from_ptr(*args.argv.offset(1)).to_bytes() }, b"foo");
            assert_eq!(unsafe { CStr::from_ptr(*args.argv.offset(2)).to_bytes() }, b"bar");
        });
    }
}
