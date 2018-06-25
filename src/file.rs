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
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NO/T
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

extern crate base64;
extern crate libc;
extern crate nereon;
extern crate serde_json;

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::prelude::PermissionsExt;

#[derive(Deserialize, Debug)]
pub struct File {
    pub path: String,
    pub user: Option<String>,
    pub group: Option<String>,
    pub mode: Option<String>,
    pub content: Option<String>,
    #[serde(default = "encoding_identity")]
    pub encoding: Encoding,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Identity,
    Base64,
}

fn encoding_identity() -> Encoding {
    Encoding::Identity
}

impl File {
    pub fn decode(&self) -> Result<Option<Vec<u8>>, String> {
        match self.content {
            Some(ref content) => match self.encoding {
                Encoding::Identity => Ok(None),
                Encoding::Base64 => match base64::decode(&content) {
                    Ok(content) => Ok(Some(content)),
                    Err(e) => Err(format!("{:?}", e)),
                },
            },
            None => Ok(None),
        }
    }

    pub fn update(&self, decoded_content: &Option<Vec<u8>>) -> io::Result<()> {
        match self.content {
            Some(ref content) => {
                match decoded_content {
                    Some(c) => fs::write(&self.path, c.as_slice()),
                    None => fs::write(&self.path, content.as_bytes()),
                }?;
                if let Some(m) = &self.mode {
                    chmod(&self.path, m)?;
                }
                if let Some(u) = &self.user {
                    chown(&self.path, &u)?;
                }
                if let Some(g) = &self.group {
                    chgrp(&self.path, &g)?;
                }
                Ok(())
            }
            None => match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(e) => match e.kind() {
                    io::ErrorKind::NotFound => Ok(()),
                    _ => Err(e),
                },
            },
        }
    }
}

pub fn parse_fileset(src: &mut io::Read) -> Result<Vec<(String, File, Option<Vec<u8>>)>, String> {
    fn decode_fileset(
        fileset: &mut HashMap<String, File>,
    ) -> Result<Vec<(String, File, Option<Vec<u8>>)>, String> {
        // build Vec of (id, File, decoded_content) tuples
        let mut result = vec![];
        for (id, f) in fileset.drain() {
            match f.decode() {
                Ok(dc) => result.push((id, f, dc)),
                Err(e) => return Err(e),
            };
        }
        Ok(result)
    }

    // convert UCL into JSON
    match nereon::libucl::ucl_to_json(src).map_err(|e| format!("{:?}", e)) {
        Ok(json) => {
            // convert JSON into Serde::Object
            match serde_json::from_str::<HashMap<String, HashMap<String, File>>>(&json) {
                Ok(mut object) => {
                    // decode "file" member if available or vec![]
                    decode_fileset(&mut object.remove("file").unwrap_or_default())
                }
                Err(e) => Err(format!("{}", e)),
            }
        }
        Err(e) => Err(e),
    }.map_err(|e| format!("Unable to parse fileset: {}", e))
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
        let passwd = unsafe { libc::getpwnam(CString::new(user.to_owned()).unwrap().as_ptr()) };
        match !passwd.is_null() {
            true => Ok(unsafe { (*passwd).pw_uid }),
            false => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("No such user {}", user),
            )),
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
        let gr = unsafe { libc::getgrnam(CString::new(group.to_owned()).unwrap().as_ptr()) };
        match !gr.is_null() {
            true => Ok(unsafe { (*gr).gr_gid }),
            false => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("No such group {}", group),
            )),
        }
    }

    let gid = get_gid(group)?;
    let path = CString::new(path.to_owned()).unwrap();
    match unsafe { libc::chown(path.as_ptr(), -1 as i32 as libc::gid_t, gid) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}
