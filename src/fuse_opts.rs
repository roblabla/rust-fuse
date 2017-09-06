

//
// FUSE arguments (see fuse_opt.h for details)
//

use libc::{c_char, c_int};

#[repr(C)]
#[derive(Debug)]
pub struct fuse_args {
    pub argc: c_int,
    pub argv: *const *const c_char,
    pub allocated: c_int,
}

cfg_if! {
    if #[cfg(feature="rust-mount")] {

        use libc::{mode_t, uid_t, gid_t};
        use getopts::Options;
        use std::str::FromStr;
        use std::ffi::CStr;
        use std::slice;

        #[derive(Copy, Clone, Debug, PartialEq)]
        pub struct ParseError;

        pub enum MetaFuseOpt {
            Uid(uid_t),
            Gid(gid_t),
            RootMode(mode_t),
            AllowOther,
            DefaultPermissions,
            NonEmpty,
            SubType(String),
            #[cfg(target_os = "darwin")]
            Volname(String), // osxfuse option
        }

        impl FromStr for MetaFuseOpt {
            type Err = ParseError;

            fn from_str(s :&str) -> Result<Self, Self::Err> {
                let (a, b) = s.split_at(s.find("=").unwrap_or(s.len()));

                match (a, String::from(b.trim_left_matches('='))) {
                    ("user_id", val) => Ok(MetaFuseOpt::Uid(val.parse::<uid_t>().unwrap())),
                    ("group_id", val) => Ok(MetaFuseOpt::Gid(val.parse::<gid_t>().unwrap())),
                    ("rootmode", val) => Ok(MetaFuseOpt::RootMode(val.parse::<mode_t>().unwrap())),
                    ("allow_other", _) => Ok(MetaFuseOpt::AllowOther),
                    ("default_permissions", _) => Ok(MetaFuseOpt::DefaultPermissions),
                    ("nonempty", _) => Ok(MetaFuseOpt::NonEmpty),
                    ("subtype", val) => Ok(MetaFuseOpt::SubType(val.clone())),
                    #[cfg(target_os = "darwin")]
                    ("volname", val) => Ok(MetaFuseOpt::Volname(val.clone())),
                    _ => Err(ParseError),
                }
            }
        }

        impl ToString for MetaFuseOpt {
            fn to_string(&self) -> String {
                match *self {

                    MetaFuseOpt::Uid(val) => format!("user_id={}", val),
                    MetaFuseOpt::Gid(val) => format!("group_id={}", val),
                    MetaFuseOpt::RootMode(val) => format!("rootmode={}", val),
                    MetaFuseOpt::AllowOther => String::from("allow_other"),
                    MetaFuseOpt::DefaultPermissions => String::from("default_permissions"),
                    MetaFuseOpt::NonEmpty => String::from("nonempty"),
                    MetaFuseOpt::SubType(ref val) => format!("subtype={}", val),
                    #[cfg(target_os = "darwin")]
                    MetaFuseOpt::Volname(ref val) => format!("volname={}", val),
                }
            }
        }

        pub struct FuseOpts {
            pub opts: Vec<MetaFuseOpt>,
        }

        impl ToString for FuseOpts {
            fn to_string(&self) -> String {
                self.opts.iter().map(|x| x.to_string()).collect::<Vec<String>>().join(",")
            }
        }

        impl FuseOpts {
            pub fn new() -> FuseOpts {
                FuseOpts {
                    opts: Vec::new(),
                }
            }

            pub fn fuse_opt_parse(&mut self, args: &fuse_args) {
                let mut opts = Options::new();
                opts.optmulti("o", "option", "", "FILE");

                let argv: Vec<&str> = unsafe {
                    let paths: &[*const _] = slice::from_raw_parts(args.argv, args.argc as usize);
                    paths.iter().map(
                        |cs| CStr::from_ptr(*cs).to_str().expect("Error convert argv")
                        ).collect()
                };

                let matches = opts.parse(argv.iter()).unwrap();
                let optss: _ = matches.opt_strs("o");
                let opts = optss.iter().map(|x| x.split(",").collect()).collect::<Vec<Vec<_>>>().concat();
                self.opts.extend(opts.iter().map(|x| MetaFuseOpt::from_str(x.trim()).unwrap()).collect::<Vec<MetaFuseOpt>>());
            }
        }
    }
}
