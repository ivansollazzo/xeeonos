//! `xctl firewall`: the firewall (nftables).
//!
//! Closed to incoming connections by default: only answers to connections
//! this computer opened, the computer talking to itself (loopback), ping and
//! the network's automatic configuration (DHCP) come in. Going out is free.
//!
//! Ports can be opened one by one (`xctl firewall allow 22/tcp`); the list is
//! /etc/xeeonos/firewall-allow, one "port/protocol" per line. At boot
//! xeeonos-firewall.service loads the rules (`xctl firewall load`).

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::util::{Result, err, need_root, output};

/// The ports opened by the administrator.
const ALLOW_FILE: &str = "/etc/xeeonos/firewall-allow";

pub fn run_firewall(args: &[String]) -> Result<()> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["status"] => status(),
        ["load"] => {
            need_root("firewall")?;
            load()
        }
        ["allow", port] => {
            need_root("firewall")?;
            let port = parse_port(port)?;
            let mut list = allowed()?;
            if !list.contains(&port) {
                list.push(port.clone());
            }
            save(&list)?;
            load()?;
            println!("Open for incoming connections: {port}");
            Ok(())
        }
        ["deny", port] => {
            need_root("firewall")?;
            let port = parse_port(port)?;
            let list: Vec<String> = allowed()?.into_iter().filter(|p| *p != port).collect();
            save(&list)?;
            load()?;
            println!("Closed: {port}");
            Ok(())
        }
        _ => Err(err(
            "usage: xctl firewall [status | allow <port>/<tcp|udp> | deny <port>/<tcp|udp>]",
        )),
    }
}

fn status() -> Result<()> {
    let list = allowed()?;
    let active = output("systemctl", &["is-active", "xeeonos-firewall.service"])
        .map(|s| s.trim() == "active")
        .unwrap_or(false);
    println!(
        "Firewall:    {}",
        if active {
            "on (incoming connections closed)"
        } else {
            "off"
        }
    );
    if list.is_empty() {
        println!("Open ports:  none");
    } else {
        println!("Open ports:  {}", list.join(" "));
    }
    Ok(())
}

/// "22/tcp" -> "22/tcp"; checks the number and the protocol.
fn parse_port(text: &str) -> Result<String> {
    let (number, proto) = text.split_once('/').ok_or_else(|| {
        err(format!(
            "write the port as <number>/tcp or <number>/udp, not {text}"
        ))
    })?;
    let n: u16 = number
        .parse()
        .map_err(|_| err(format!("not a port number: {number}")))?;
    if n == 0 || !matches!(proto, "tcp" | "udp") {
        return Err(err(format!("not a valid port: {text}")));
    }
    Ok(format!("{n}/{proto}"))
}

fn allowed() -> Result<Vec<String>> {
    let text = fs::read_to_string(ALLOW_FILE).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(parse_port)
        .collect()
}

fn save(list: &[String]) -> Result<()> {
    if let Some(dir) = Path::new(ALLOW_FILE).parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = String::from("# Ports open for incoming connections (xctl firewall).\n");
    for p in list {
        text.push_str(p);
        text.push('\n');
    }
    fs::write(ALLOW_FILE, text)?;
    Ok(())
}

/// The nftables rules for the given open ports.
fn ruleset(allowed: &[String]) -> String {
    let ports = |proto: &str| -> Vec<&str> {
        allowed
            .iter()
            .filter_map(|p| p.strip_suffix(&format!("/{proto}")))
            .collect()
    };
    let mut open = String::new();
    for proto in ["tcp", "udp"] {
        let list = ports(proto);
        if !list.is_empty() {
            open.push_str(&format!(
                "        {proto} dport {{ {} }} accept\n",
                list.join(", ")
            ));
        }
    }
    format!(
        "destroy table inet xeeonos\n\
         table inet xeeonos {{\n\
         \x20   chain input {{\n\
         \x20       type filter hook input priority filter; policy drop;\n\
         \x20       ct state established,related accept\n\
         \x20       ct state invalid drop\n\
         \x20       iif lo accept\n\
         \x20       meta l4proto {{ icmp, ipv6-icmp }} accept\n\
         \x20       udp dport 68 accept comment \"DHCP\"\n\
         \x20       udp sport 547 udp dport 546 accept comment \"DHCPv6\"\n\
         {open}\
         \x20   }}\n\
         }}\n"
    )
}

/// Replace the rules in the kernel with the current ones.
fn load() -> Result<()> {
    let rules = ruleset(&allowed()?);
    let mut child = Command::new("nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| err(format!("cannot run nft: {e}")))?;
    child
        .stdin
        .take()
        .ok_or_else(|| err("nft: no input"))?
        .write_all(rules.as_bytes())?;
    if !child.wait()?.success() {
        return Err(err("nft refused the firewall rules"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports() {
        assert_eq!(parse_port("22/tcp").unwrap(), "22/tcp");
        assert!(parse_port("22").is_err());
        assert!(parse_port("0/tcp").is_err());
        assert!(parse_port("70000/udp").is_err());
        assert!(parse_port("22/icmp").is_err());
    }

    #[test]
    fn rules() {
        let r = ruleset(&["22/tcp".into(), "8080/tcp".into(), "5353/udp".into()]);
        assert!(r.contains("policy drop;"));
        assert!(r.contains("tcp dport { 22, 8080 } accept"));
        assert!(r.contains("udp dport { 5353 } accept"));
        assert!(!ruleset(&[]).contains("dport {"));
    }
}
