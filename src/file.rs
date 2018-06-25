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
    pub fn write(&self, id: &str) -> Result<(), String> {
        match self.content {
            Some(ref content) => {
                let decoded_content;
                let content = match self.encoding {
                    Encoding::Identity => Ok(content.as_bytes()),
                    Encoding::Base64 => match base64::decode(content) {
                        Ok(content) => {
                            decoded_content = content;
                            Ok(decoded_content.as_slice())
                        }
                        Err(e) => Err(format!("{}. {}", e, id)),
                    },
                };
                match content {
                    Ok(content) => match self.write_file(content) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(format!("Failed to write file {}: {}", id, e)),
                    },
                    Err(e) => Err(e),
                }
            }
            None => match fs::remove_file(&self.path) {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Failed to remove file {}: {:?}", id, e)),
            },
        }
    }

    fn write_file(&self, content: &[u8]) -> Result<(), String> {
        let write_result = fs::write(&self.path, content.as_ref());

        let chmod_result = match &self.mode {
            Some(m) => chmod(&self.path, m),
            None => Ok(()),
        };
        let chown_result = match &self.user {
            Some(u) => chown(&self.path, &u),
            None => Ok(()),
        };
        let chgrp_result = match &self.group {
            Some(g) => chgrp(&self.path, &g),
            None => Ok(()),
        };
        match (write_result, chmod_result, chown_result, chgrp_result) {
            (Ok(_), Ok(_), Ok(_), Ok(_)) => Ok(()),
            _ => Err("Write failed".to_owned()),
        }
    }
}

pub fn parse_fileset(src: &mut io::Read) -> Result<HashMap<String, File>, String> {
    // convert fileset into json and parse into a Fileset
    match nereon::libucl::ucl_to_json(src) {
        Ok(s) => match serde_json::from_str::<HashMap<String, HashMap<String, File>>>(&s) {
            Ok(mut s) => match s.remove("file") {
                Some(f) => Ok(f),
                None => Ok(HashMap::new()),
            },
            Err(e) => Err(format!("Not a valid fileset: {}", e)),
        },
        Err(e) => Err(format!("Not a valid fileset: {:?}", e)),
    }
}

fn chmod(path: &str, mode: &str) -> io::Result<()> {
    match format!("0o{}", mode).parse::<u32>() {
        Ok(n) if n < 0o10000 => fs::set_permissions(path, fs::Permissions::from_mode(n)),
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
