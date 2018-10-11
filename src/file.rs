// Copyright (c) 2018, [Ribose Inc](https://www.ribose.com).
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use base64;
use libc;
use nereon::{self, FromValue, Value};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::prelude::PermissionsExt;

#[derive(FromValue, Debug)]
pub struct File {
    pub path: String,
    pub user: Option<String>,
    pub group: Option<String>,
    pub mode: Option<String>,
    pub content: Option<String>,
    pub encoding: Option<Encoding>,
}

#[derive(FromValue, Debug)]
pub enum Encoding {
    Base64,
}

impl File {
    pub fn decode(&self) -> Result<Vec<u8>, String> {
        self.content.as_ref().map_or_else(
            || Ok(Vec::new()),
            |ref content| {
                self.encoding.as_ref().map_or_else(
                    || Ok(content.bytes().collect()),
                    |encoding| match encoding {
                        Encoding::Base64 => {
                            base64::decode(&content).map_err(|e| format!("{:?}", e))
                        }
                    },
                )
            },
        )
    }

    pub fn update(&self, decoded_content: &[u8]) -> io::Result<()> {
        self.content.as_ref().map_or_else(
            || {
                fs::remove_file(&self.path).or_else(|e| match e.kind() {
                    io::ErrorKind::NotFound => Ok(()),
                    _ => Err(e),
                })
            },
            |_| {
                let update = |v: &Option<String>, f: fn(&str, &str) -> io::Result<()>| {
                    v.as_ref().map_or_else(|| Ok(()), |v| f(&self.path, v))
                };
                fs::write(&self.path, decoded_content)
                    .and_then(|_| update(&self.mode, chmod))
                    .and_then(|_| update(&self.user, chown))
                    .and_then(|_| update(&self.group, chgrp))
            },
        )
    }
}

pub fn parse_fileset(src: &mut io::Read) -> Result<HashMap<String, (File, Vec<u8>)>, String> {
    let mut fileset = String::new();
    src.read_to_string(&mut fileset)
        .map_err(|e| format!("{:?}", e))?;

    nereon::parse_noc::<HashMap<String, HashMap<String, File>>>(&fileset)
        .map_err(|e| format!("{:?}", e))
        .and_then(|mut fileset| {
            fileset.get_mut("file").map_or_else(
                || Ok(HashMap::new()),
                |fileset| {
                    fileset.drain().try_fold(HashMap::new(), |mut v, (id, f)| {
                        f.decode().map(|d| {
                            v.insert(id, (f, d));
                            v
                        })
                    })
                },
            )
        })
}

fn chmod(path: &str, mode: &str) -> io::Result<()> {
    match isize::from_str_radix(mode, 8) {
        Ok(n) if n > 0 && n < 0o10000 => {
            fs::set_permissions(path, fs::Permissions::from_mode(n as u32))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Not a valid mode {}", mode),
        )),
    }
}

fn chown(path: &str, user: &str) -> io::Result<()> {
    fn get_uid(user: &str) -> io::Result<u32> {
        let cuser = CString::new(user.to_owned()).unwrap();
        let passwd = unsafe { libc::getpwnam(cuser.as_ptr()) };
        if !passwd.is_null() {
            Ok(unsafe { (*passwd).pw_uid })
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("No such user {}", user),
            ))
        }
    }

    let uid = get_uid(user)?;
    let path = CString::new(path.to_owned()).unwrap();
    match unsafe { libc::chown(path.as_ptr(), uid, -1 as i32 as libc::gid_t) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

fn chgrp(path: &str, group: &str) -> io::Result<()> {
    fn get_gid(group: &str) -> io::Result<u32> {
        let cgroup = CString::new(group.to_owned()).unwrap();
        let gr = unsafe { libc::getgrnam(cgroup.as_ptr()) };
        if !gr.is_null() {
            Ok(unsafe { (*gr).gr_gid })
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("No such group {}", group),
            ))
        }
    }

    let gid = get_gid(group)?;
    let path = CString::new(path.to_owned()).unwrap();
    match unsafe { libc::chown(path.as_ptr(), -1 as i32 as libc::gid_t, gid) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}
