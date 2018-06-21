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
extern crate nereon;
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Config {
    fileset_file: Option<String>,
}

#[derive(Deserialize, Debug)]
struct NereonFile {
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
        Some(n) => match File::open(&n) {
            Ok(mut f) => {
                let mut s = String::new();
                match f.read_to_string(&mut s) {
                    Ok(_) => s,
                    Err(e) => fail(format!("Failed to read fileset file {}: {:?}.", n, e)),
                }
            }
            Err(e) => fail(format!("Couldn't open fileset file {}; {:?}.", n, e)),
        },
        _ => {
            match env::var("NEREON_FILESET") {
                Ok(s) => {
                    // env var is base64 encoded
                    match base64_decode(&s) {
                        Ok(s) => s,
                        Err(e) => fail(format!("{} in env[NEREON_FILESET].", e)),
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
            match serde_json::from_str::<HashMap<String, HashMap<String, NereonFile>>>(&s) {
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
                let content = match Encoding::Identity {
                    Encoding::Identity => Ok(content),
                    Encoding::Base64 => match base64_decode(content) {
                        Ok(content) => {
                            decoded_content = content;
                            Ok(&decoded_content)
                        }
                        Err(e) => Err(format!("{} found in content for {}", e, id)),
                    },
                };
                match content {
                    Ok(content) => match write_file(&Path::new(&f.path), &content, &f.mode) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(format!("Failed to write file {}: {}", id, e)),
                    },
                    Err(e) => Err(e),
                }
            }
            None => match delete_file(&Path::new(&f.path)) {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            },
        };

        if let Err(e) = result {
            eprintln!("{}", e);
        }
    }
}

fn delete_file(path: &Path) -> Result<(), String> {
    Ok(())
}

fn write_file(path: &Path, content: &str, mode: &Option<String>) -> Result<(), String> {
    Ok(())
}

fn full_path(path: &mut Path) -> Result<PathBuf, String> {
    let path = path.to_path_buf();
    match path.is_relative() {
        true => {
            match env::current_dir() {
                Ok(p) => Ok(PathBuf::from(p).join(&path)),
                Err(_) => Err("Couldn't get canonical file path".to_owned()),
            }
        },
        false => Ok(path)
    }
}

fn base64_decode(s: &str) -> Result<String, String> {
    match base64::decode(s) {
        Ok(bs) => match String::from_utf8(bs) {
            Ok(s) => Ok(s),
            Err(_) => Err("Invalid utf8 data".to_owned()),
        },
        Err(_) => Err("Invalid base64 data".to_owned()),
    }
}

fn fail<T>(e: String) -> T {
    eprintln!("{}: {}", env::args().next().unwrap(), e);
    std::process::exit(1);
}
