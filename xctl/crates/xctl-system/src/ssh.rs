//! `xctl ssh-server`: the SSH server (module tools, docs/tools.md).
//!
//! Installed but off: the service sshd is disabled and the firewall keeps
//! its port (22/tcp) closed. Turning it on does both things together, so
//! the server is never running behind a closed port, nor the port open with
//! nothing behind it.

use crate::firewall;
use crate::util::{Result, err, need_root, run};

const SERVICE: &str = "sshd.service";
const PORT: &str = "22/tcp";

pub fn run_ssh_server(args: &[String]) -> Result<()> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["status"] => status(),
        ["on"] => {
            need_root("ssh-server on")?;
            run("systemctl", &["enable", "--now", SERVICE])?;
            set_port(true)?;
            println!("SSH server: on (port {PORT} open)");
            Ok(())
        }
        ["off"] => {
            need_root("ssh-server off")?;
            set_port(false)?;
            run("systemctl", &["disable", "--now", SERVICE])?;
            println!("SSH server: off (port {PORT} closed)");
            Ok(())
        }
        _ => Err(err("usage: xctl ssh-server [on|off]")),
    }
}

fn set_port(open: bool) -> Result<()> {
    let mut list: Vec<String> = firewall::allowed()?
        .into_iter()
        .filter(|p| p != PORT)
        .collect();
    if open {
        list.push(PORT.to_string());
    }
    firewall::save(&list)?;
    firewall::load()
}

fn status() -> Result<()> {
    // (is-active and is-enabled exit with an error when the answer is no:
    // only their output counts.)
    let state = |verb: &str| {
        std::process::Command::new("systemctl")
            .args([verb, SERVICE])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let open = firewall::allowed()?.iter().any(|p| p == PORT);
    let running = state("is-active") == "active";
    let enabled = state("is-enabled") == "enabled";
    let summary = match (running || enabled, open) {
        (true, true) => "on",
        (false, false) => "off",
        (true, false) => "on, but its port is closed in the firewall",
        (false, true) => "off, but its port is open in the firewall",
    };
    println!("SSH server:  {summary}");
    println!("Turn on or off with: sudo xctl ssh-server on|off");
    Ok(())
}
