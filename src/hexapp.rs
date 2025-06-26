use crate::applet::Applet;
use crate::applet::SliceExt;
use anyhow::{Context, Result};
use clap::{arg, Command};

pub struct HexApplet {}

impl Applet for HexApplet {
    fn command(&self) -> &'static str {
        "hex"
    }
    fn description(&self) -> &'static str {
        "hex encode"
    }

    fn parse_args(&self, _args: &clap::ArgMatches) -> Result<Box<dyn Applet>> {
        Ok(Box::new(Self {}))
    }

    fn process(&self, val: Vec<u8>) -> Result<Vec<u8>> {
        Ok(hex::encode(val).as_bytes().to_vec())
    }

    fn new() -> Box<dyn Applet> {
        Box::new(Self {})
    }
}

pub struct UnHexApplet {
    hexonly: bool,
    strict: bool,
}

impl UnHexApplet {
    fn hex_decode_hexonly(&self, val: Vec<u8>) -> Result<Vec<u8>> {
        let mut trimmed: Vec<u8> = val.trim().into();
        let res = hex::decode(&trimmed);
        if self.strict {
            return res.with_context(|| "Invalid hex input");
        }
        /* remove spaces */
        trimmed.retain(|&x| x != 0x20);
        let res = hex::decode(&trimmed);
        match res {
            Ok(decoded) => Ok(decoded),
            Err(e) => match e {
                hex::FromHexError::InvalidHexCharacter { c: _, index } => {
                    let mut end = trimmed.split_off(index);
                    let mut decoded = self.hex_decode_hexonly(trimmed)?;
                    decoded.append(&mut end);
                    Ok(decoded)
                }
                hex::FromHexError::OddLength => {
                    // TODO: refactor
                    let mut end = trimmed.split_off(trimmed.len() - 1);
                    let mut decoded = self.hex_decode_hexonly(trimmed)?;
                    decoded.append(&mut end);
                    Ok(decoded)
                }
                _ => panic!("{}", e),
            },
        }
    }

    fn hex_decode_all(&self, hexval: Vec<u8>) -> Result<Vec<u8>> {
        let mut res: Vec<u8> = vec![];
        let iter = &mut hexval.windows(2);
        let mut last: &[u8] = &[];
        loop {
            let chro = iter.next();
            let chr = match chro {
                None => {
                    res.extend_from_slice(last);
                    return Ok(res);
                }
                Some(a) => a,
            };

            if (chr[0] as char).is_ascii_hexdigit() && (chr[1] as char).is_ascii_hexdigit() {
                res.append(&mut hex::decode(chr).with_context(|| "hex decoding failed")?);
                /* make sure we dont miss the last char if we have something like
                 * "41 " as input */
                let next_win = iter.next().unwrap_or(&[]);
                if next_win.len() > 1 {
                    last = &next_win[1..2]
                } else {
                    last = &[]
                };
            } else {
                res.extend_from_slice(&chr[0..1]);
                last = &chr[1..2];
            }
        }
    }
}

impl Applet for UnHexApplet {
    fn command(&self) -> &'static str {
        "unhex"
    }
    fn description(&self) -> &'static str {
        "hex decode"
    }

    fn new() -> Box<dyn Applet> {
        Box::new(Self {
            hexonly: false,
            strict: false,
        })
    }

    fn clap_command(&self) -> Command {
        Command::new(self.command()).about(self.description())
             .arg(arg!(-o --"hex-only"  "expect only hex data, stop at first non-hex byte (but copy the rest, except spaces)"))
             .arg(arg!(-s --strict  "strict decoding, error on invalid data"))
             .arg(arg!([value]  "input value, reads from stdin if not present"))
             .after_help("By default, decode all hex data in the input, regardless of garbage in-between.")
    }

    fn parse_args(&self, args: &clap::ArgMatches) -> Result<Box<dyn Applet>> {
        Ok(Box::new(Self {
            hexonly: args.get_flag("hex-only") || args.get_flag("strict"),
            strict: args.get_flag("strict"),
        }))
    }

    fn process(&self, val: Vec<u8>) -> Result<Vec<u8>> {
        if self.hexonly {
            self.hex_decode_hexonly(val)
        } else {
            self.hex_decode_all(val)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_cli_arg() {
        assert_cmd::Command::cargo_bin("rsbkb")
            .expect("Could not run binary")
            .args(&["hex", "aAé!"])
            .assert()
            .stdout("6141c3a921")
            .success();
    }

    #[test]
    fn test_hex_cli_stdin() {
        assert_cmd::Command::cargo_bin("rsbkb")
            .expect("Could not run binary")
            .args(&["hex"])
            .write_stdin("aAé!\n")
            .assert()
            .stdout("6141c3a9210a")
            .success();
    }

    #[test]
    fn test_unhex_cli_arg() {
        assert_cmd::Command::cargo_bin("rsbkb")
            .expect("Could not run binary")
            .args(&["unhex", "6141210a00ff"])
            .assert()
            .stdout(&b"aA!\n\x00\xff"[..])
            .success();
    }

    #[test]
    fn test_unhex_cli_stdin() {
        assert_cmd::Command::cargo_bin("rsbkb")
            .expect("Could not run binary")
            .args(&["unhex"])
            .write_stdin("41ff\n00FF")
            .assert()
            .stdout(&[0x41, 0xFF, 0x0A, 0x00, 0xFF][..])
            .success();
    }

    #[test]
    fn test_unhex_cli_stdin_hexonly() {
        assert_cmd::Command::cargo_bin("rsbkb")
            .expect("Could not run binary")
            .args(&["unhex", "-o"])
            .write_stdin("41ff\n00FF")
            .assert()
            .stdout(&b"A\xFF\n00FF"[..])
            .success();
    }

    #[test]
    fn test_unhex_cli_stdin_strict() {
        assert_cmd::Command::cargo_bin("rsbkb")
            .expect("Could not run binary")
            .args(&["unhex", "-s"])
            .write_stdin("41l")
            .assert()
            .stdout(&b""[..])
            .stderr(predicates::str::contains("Odd number of digits"))
            .failure();
        assert_cmd::Command::cargo_bin("rsbkb")
            .expect("Could not run binary")
            .args(&["unhex", "-s"])
            .write_stdin("41ll")
            .assert()
            .stdout(&b""[..])
            .stderr(predicates::str::contains("Invalid character"))
            .failure();
    }

    #[test]
    fn test_hex() {
        let hex = HexApplet {};
        assert_eq!(
            String::from_utf8(hex.process_test([0, 0xFF].to_vec())).unwrap(),
            "00ff"
        );
    }

    #[test]
    fn test_unhex_hexonly() {
        let unhex = UnHexApplet {
            strict: false,
            hexonly: true,
        };
        assert_eq!(
            unhex
                .process("01 23 45 67 89 ab cd ef".as_bytes().to_vec())
                .unwrap(),
            [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
        );
        assert_eq!(
            unhex
                .process("0123456789abcdef".as_bytes().to_vec())
                .unwrap(),
            [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
        );
    }

    #[test]
    fn test_unhex() {
        let unhex = UnHexApplet {
            strict: false,
            hexonly: false,
        };
        assert_eq!(
            unhex.process("test52af ".as_bytes().to_vec()).unwrap(),
            [0x74, 0x65, 0x73, 0x74, 0x52, 0xaf, 0x20]
        );
        assert_eq!(
            unhex.process("test52af".as_bytes().to_vec()).unwrap(),
            [0x74, 0x65, 0x73, 0x74, 0x52, 0xaf]
        );
        assert_eq!(
            unhex.process("!52af".as_bytes().to_vec()).unwrap(),
            [0x21, 0x52, 0xaf]
        );
        assert_eq!(
            unhex.process("!5 2af".as_bytes().to_vec()).unwrap(),
            [0x21, 0x35, 0x20, 0x2a, 0x66]
        );
    }
}
