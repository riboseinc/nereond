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

extern crate base64;
extern crate libc;
extern crate nereon;

#[macro_use]
extern crate nereon_derive;

#[macro_use]
extern crate log;

mod file;

use nereon::{FromValue, Value};
use std::env;
use std::fs;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const LICENSE: &str = "BSD-2-Clause";
const APPNAME: &str = env!("CARGO_PKG_NAME");

#[derive(FromValue)]
struct Config {
    fileset_file: Option<String>,
    fileset_env: Option<String>,
}

pub fn nereond() -> Result<(), String> {
    let nos = format!(
        r#"
        authors ["{}"]
        license "{}"
        name "{}"
        version {}
        option fileset_file {{
            flags [takesvalue]
            short f
            long fileset
            env NEREON_FILESET_FILE
            hint FILE
            usage "File containing a nereon fileset"
            key [fileset_file]
        }},
        option fileset {{
            env NEREON_FILESET
            hint FILE
            usage "Fileset as environment variable"
            key [fileset_env]
        }}"#,
        AUTHORS, LICENSE, APPNAME, VERSION
    );

    let config = nereon::configure::<Config, _, _>(&nos, env::args()).unwrap();

    // get the fileset from file/env
    config
        .fileset_file
        .as_ref()
        .map_or_else(
            || {
                config.fileset_env.as_ref().map_or_else(
                    || Err("No fileset from args or environment.".to_owned()),
                    |s| {
                        base64::decode(&s)
                            .map_err(|_| format!("Invalid base64 data in env[NEREON_FILESET] {}", s))
                            .and_then(|bs| {
                                String::from_utf8(bs).map_err(|_| {
                                    "Invalid utf8 data in env[NEREON_FILESET]".to_owned()
                                })
                            })
                    },
                )
            },
            |file| {
                fs::read_to_string(&file)
                    .map_err(|e| format!("Failed to read fileset file {}: {:?}.", file, e))
            },
        ).and_then(|fileset| file::parse_fileset(&mut fileset.as_bytes()))
        .map(|fileset| {
            fileset.iter().for_each(|(id, (f, decoded_content))| {
                f.update(decoded_content)
                    .map_err(|e| warn!("Failed to update {}: {}", id, e))
                    .ok();
            })
        })

    // At this point we're initialized: ie written an initial set of configs.
    // To signal this we either fork and continue to listen for configuration updates
    // or simply exit.

    // Either way, control should return to the process spawner which now can start
    // any processes dependent on nereond for configuration.
}
