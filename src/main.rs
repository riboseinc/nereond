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
extern crate nereond;
extern crate serde_json;

use nereond::parse_fileset;
use std::env;
use std::fs;

#[macro_use]
extern crate serde_derive;

#[derive(Deserialize)]
struct Config {
    fileset_file: Option<String>,
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

    let fileset = match parse_fileset(&mut fileset.as_bytes()) {
        Ok(fs) => fs,
        Err(e) => fail(format!("{}.", e)),
    };

    // write fileset to disk
    for (id, f, decoded_content) in fileset.iter() {
        if let Err(e) = f.update(decoded_content) {
            eprintln!("Failed to update {}: {}", id, e);
        }
    }
}

fn fail<T>(e: String) -> T {
    eprintln!("{}: {}", env::args().next().unwrap(), e);
    std::process::exit(1);
}
