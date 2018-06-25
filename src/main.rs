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

#[macro_use]
extern crate serde_derive;

use std::collections::HashMap;
use std::env;
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::unix::prelude::PermissionsExt;

#[derive(Deserialize)]
struct Config {
    fileset_file: Option<String>,
}

#[derive(Deserialize, Debug)]
struct File {
    path: String,
    user: Option<String>,
    group: Option<String>,
    mode: Option<String>,
    content: Option<String>,
    #[serde(default = "encoding_identity")]
    encoding: Encoding,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum Encoding {
    Identity,
    Base64,
}

fn encoding_identity() -> Encoding {
    Encoding::Identity
}

fn main() {
    let options = vec![nereon::Opt::new(
        "fileset_file",
        Some("f"),
        Some("fileset"),
        Some("NEREON_FILESET_FILE"),
        nereon::OptFlag::Optional as u32,
        None,
        None,
        Some("File containing a nereon fileset"),
    )];

    let config = match nereon::nereon_json(options, env::args()) {
        Ok(c) => c,
        Err(s) => fail(format!("Couldn't get config: {}", s)),
    };

    let config = match serde_json::from_str::<Config>(&config) {
        Ok(c) => c,
        Err(e) => fail(format!("Invalid config: {}", e)),
    };

    // get the fileset from file/env
    let fileset = match config.fileset_file {
        Some(n) => match fs::read_to_string(&n) {
            Ok(s) => s,
            Err(e) => fail(format!("Failed to read fileset file {}: {:?}.", n, e)),
        },
        _ => {
            match env::var("NEREON_FILESET") {
                Ok(s) => {
                    // env var is base64 encoded
                    match base64::decode(&s) {
                        Ok(bs) => match String::from_utf8(bs) {
                            Ok(s) => s,
                            Err(_) => fail("Invalid utf8 data in env[NEREON_FILESET]".to_owned()),
                        },
                        Err(_) => fail("Invalid base64 data in env[NEREON_FILESET]".to_owned()),
                    }
                }
                Err(_) => fail("No fileset from args or environment.".to_owned()),
            }
        }
    };

    // convert fileset into json and parse into a Fileset
    let fileset = match nereon::libucl::ucl_to_json(&mut fileset.as_bytes()) {
        Ok(s) => {
            println!("{:?}", s);
            match serde_json::from_str::<HashMap<String, HashMap<String, File>>>(&s) {
                Ok(mut s) => match s.remove("file") {
                    Some(f) => f,
                    _ => HashMap::new(),
                },
                Err(e) => fail(format!("Not a valid fileset: {}", e)),
            }
        }
        Err(e) => fail(format!("Not a valid fileset: {:?}", e)),
    };
    println!("{:?}", fileset);

    // write fileset to disk
    for (id, f) in fileset.iter() {
        let result = match f.content {
            Some(ref content) => {
                // create/overwrite file
                let decoded_content;
                let content = match f.encoding {
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
                    Ok(content) => match write_file(&f, content) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(format!("Failed to write file {}: {}", id, e)),
                    },
                    Err(e) => Err(e),
                }
            }
            None => match fs::remove_file(&f.path) {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Failed to remove file {}: {:?}", id, e)),
            },
        };

        if let Err(e) = result {
            eprintln!("{}", e);
        }
    }
}

fn write_file(file: &File, content: &[u8]) -> Result<(), String> {
    let write_result = fs::write(&file.path, content.as_ref());

    let chmod_result = match &file.mode {
        Some(m) => chmod(&file.path, m),
        None => Ok(()),
    };
    let chown_result = match &file.user {
        Some(u) => chown(&file.path, &u),
        None => Ok(()),
    };
    let chgrp_result = match &file.group {
        Some(g) => chgrp(&file.path, &g),
        None => Ok(()),
    };
    match (write_result, chmod_result, chown_result, chgrp_result) {
        (Ok(_), Ok(_), Ok(_), Ok(_)) => Ok(()),
        _ => Err("Write failed".to_owned()),
    }
}

fn fail<T>(e: String) -> T {
    eprintln!("{}: {}", env::args().next().unwrap(), e);
    std::process::exit(1);
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
