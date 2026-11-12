//! `xctl network`: wired and Wi-Fi connections, automatic (DHCP) or manual
//! addresses. The same screen is used by the first-boot setup and by
//! Recovery. Everything goes through NetworkManager (`nmcli`), so the
//! settings are saved in /etc/NetworkManager and the desktop sees them too.
//!
//! Without a terminal (tests, scripts):
//!
//! ```text
//! xctl network status
//! xctl network ethernet [--device <dev>] (--dhcp | --address <ip/prefix> --gateway <ip> [--dns <ip,ip>])
//! xctl network wifi <name> [--password <pw>] [--device <dev>] [same address options]
//! ```

use std::net::{IpAddr, Ipv4Addr};
use std::process::Command;

use crate::ui;
use crate::util::{Result, err, need_root, output, run};

/// How a connection gets its address.
#[derive(Debug, PartialEq)]
pub enum Ipv4 {
    Dhcp,
    Manual {
        address: String, // e.g. 192.168.1.20/24
        gateway: String,
        dns: Vec<String>,
    },
}

/// A network interface, as NetworkManager sees it.
struct Device {
    name: String,  // e.g. enp0s3, wlan0
    kind: String,  // ethernet, wifi
    state: String, // connected, disconnected, unavailable, ...
}

pub fn run_network(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None => {
            need_root("network")?;
            screen()
        }
        Some("status") => {
            print!("{}", status_text()?);
            Ok(())
        }
        Some("ethernet") => {
            need_root("network")?;
            let opts = Options::parse(&args[1..])?;
            let device = match opts.device {
                Some(d) => d,
                None => first_device("ethernet")?,
            };
            configure_ethernet(
                &device,
                &opts.ipv4.ok_or_else(|| err("give --dhcp or --address"))?,
            )
        }
        Some("wifi") => {
            need_root("network")?;
            let ssid = args
                .get(1)
                .filter(|a| !a.starts_with("--"))
                .ok_or_else(|| err("usage: xctl network wifi <name> [options]"))?;
            let opts = Options::parse(&args[2..])?;
            let device = match opts.device {
                Some(d) => d,
                None => first_device("wifi")?,
            };
            connect_wifi(
                &device,
                ssid,
                opts.password.as_deref(),
                &opts.ipv4.unwrap_or(Ipv4::Dhcp),
            )
        }
        Some(other) => Err(err(format!("unknown option for network: {other}"))),
    }
}

/// Options of `xctl network ethernet|wifi`.
struct Options {
    device: Option<String>,
    password: Option<String>,
    ipv4: Option<Ipv4>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Options> {
        let (mut device, mut password, mut dhcp) = (None, None, false);
        let (mut address, mut gateway, mut dns) = (None, None, Vec::new());
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let mut value = || {
                it.next()
                    .cloned()
                    .ok_or_else(|| err(format!("{arg} needs a value")))
            };
            match arg.as_str() {
                "--device" => device = Some(value()?),
                "--password" => password = Some(value()?),
                "--dhcp" => dhcp = true,
                "--address" => address = Some(value()?),
                "--gateway" => gateway = Some(value()?),
                "--dns" => dns = split_list(&value()?),
                other => return Err(err(format!("unknown option for network: {other}"))),
            }
        }
        let ipv4 = match (dhcp, address) {
            (true, None) => Some(Ipv4::Dhcp),
            (false, Some(a)) => {
                let gateway = gateway.ok_or_else(|| err("--address needs --gateway"))?;
                Some(manual(&a, &gateway, &dns)?)
            }
            (false, None) => None,
            (true, Some(_)) => return Err(err("--dhcp and --address do not go together")),
        };
        Ok(Options {
            device,
            password,
            ipv4,
        })
    }
}

/// Check and build manual settings.
pub fn manual(address: &str, gateway: &str, dns: &[String]) -> Result<Ipv4> {
    check_cidr(address)?;
    gateway
        .parse::<Ipv4Addr>()
        .map_err(|_| err(format!("not an IPv4 address: {gateway}")))?;
    for d in dns {
        d.parse::<IpAddr>()
            .map_err(|_| err(format!("not an IP address: {d}")))?;
    }
    Ok(Ipv4::Manual {
        address: address.to_string(),
        gateway: gateway.to_string(),
        dns: dns.to_vec(),
    })
}

/// `192.168.1.20/24`: an IPv4 address and a prefix length.
fn check_cidr(s: &str) -> Result<()> {
    let bad = || {
        err(format!(
            "not an address with prefix (e.g. 192.168.1.20/24): {s}"
        ))
    };
    let (ip, prefix) = s.split_once('/').ok_or_else(bad)?;
    ip.parse::<Ipv4Addr>().map_err(|_| bad())?;
    match prefix.parse::<u8>() {
        Ok(p) if (1..=32).contains(&p) => Ok(()),
        _ => Err(bad()),
    }
}

/// "1.1.1.1, 9.9.9.9" or "1.1.1.1 9.9.9.9" -> ["1.1.1.1", "9.9.9.9"]
fn split_list(s: &str) -> Vec<String> {
    s.split([',', ' '])
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// Split one line of `nmcli --terse` output: fields are separated by ':',
/// and a ':' inside a field is written "\:".
fn terse_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    fields.last_mut().unwrap().push(next);
                }
            }
            ':' => fields.push(String::new()),
            _ => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

fn devices() -> Result<Vec<Device>> {
    let out = output(
        "nmcli",
        &["--terse", "--fields", "DEVICE,TYPE,STATE", "device"],
    )?;
    Ok(out
        .lines()
        .map(terse_fields)
        .filter(|f| f.len() >= 3 && (f[1] == "ethernet" || f[1] == "wifi"))
        .map(|f| Device {
            name: f[0].clone(),
            kind: f[1].clone(),
            state: f[2].clone(),
        })
        .collect())
}

fn first_device(kind: &str) -> Result<String> {
    devices()?
        .into_iter()
        .find(|d| d.kind == kind)
        .map(|d| d.name)
        .ok_or_else(|| err(format!("no {kind} device found")))
}

/// The IPv4 addresses of a device, e.g. "10.0.2.15/24".
fn addresses(device: &str) -> String {
    output(
        "nmcli",
        &["--get-values", "IP4.ADDRESS", "device", "show", device],
    )
    .map(|s| {
        s.split(['|', '\n'])
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    })
    .unwrap_or_default()
}

/// A short report of every device, for `xctl network status` and the screen.
fn status_text() -> Result<String> {
    let mut text = String::new();
    for d in devices()? {
        let kind = if d.kind == "wifi" { "Wi-Fi" } else { "Wired" };
        let addr = addresses(&d.name);
        text.push_str(&format!("{kind} {}: {}", d.name, d.state));
        if !addr.is_empty() {
            text.push_str(&format!(" ({addr})"));
        }
        text.push('\n');
    }
    if text.is_empty() {
        text.push_str("No network devices found.\n");
    }
    Ok(text)
}

/// The NetworkManager connection used by a device: the active one, or a new
/// one for a wired device that has none yet.
fn wired_connection(device: &str) -> Result<String> {
    let current = output(
        "nmcli",
        &[
            "--get-values",
            "GENERAL.CONNECTION",
            "device",
            "show",
            device,
        ],
    )
    .unwrap_or_default()
    .trim()
    .to_string();
    if !current.is_empty() {
        return Ok(current);
    }
    let name = format!("Wired {device}");
    run(
        "nmcli",
        &[
            "connection",
            "add",
            "type",
            "ethernet",
            "ifname",
            device,
            "con-name",
            &name,
        ],
    )?;
    Ok(name)
}

/// Write the address settings into a connection and (re)activate it.
fn apply_ipv4(connection: &str, ipv4: &Ipv4) -> Result<()> {
    let dns_joined;
    let settings: Vec<&str> = match ipv4 {
        Ipv4::Dhcp => vec![
            "ipv4.method",
            "auto",
            "ipv4.addresses",
            "",
            "ipv4.gateway",
            "",
            "ipv4.dns",
            "",
        ],
        Ipv4::Manual {
            address,
            gateway,
            dns,
        } => {
            dns_joined = dns.join(" ");
            vec![
                "ipv4.method",
                "manual",
                "ipv4.addresses",
                address,
                "ipv4.gateway",
                gateway,
                "ipv4.dns",
                &dns_joined,
            ]
        }
    };
    let mut args = vec!["connection", "modify", connection];
    args.extend(settings);
    run("nmcli", &args)?;
    run("nmcli", &["connection", "up", connection])
}

pub fn configure_ethernet(device: &str, ipv4: &Ipv4) -> Result<()> {
    let connection = wired_connection(device)?;
    apply_ipv4(&connection, ipv4)?;
    println!("Network: {device} configured ({})", describe(ipv4));
    Ok(())
}

pub fn connect_wifi(device: &str, ssid: &str, password: Option<&str>, ipv4: &Ipv4) -> Result<()> {
    let mut args = vec!["device", "wifi", "connect", ssid, "ifname", device];
    if let Some(p) = password {
        args.extend(["password", p]);
    }
    run("nmcli", &args)?;
    // `nmcli device wifi connect` names the connection after the network.
    if *ipv4 != Ipv4::Dhcp {
        apply_ipv4(ssid, ipv4)?;
    }
    println!("Network: connected to {ssid} ({})", describe(ipv4));
    Ok(())
}

fn describe(ipv4: &Ipv4) -> String {
    match ipv4 {
        Ipv4::Dhcp => "automatic address".to_string(),
        Ipv4::Manual { address, .. } => format!("manual address {address}"),
    }
}

/// The interactive network screen. Returns when the user chooses "Done".
pub fn screen() -> Result<()> {
    loop {
        let devs = devices()?;
        let mut items: Vec<(String, String)> = devs
            .iter()
            .map(|d| {
                let kind = if d.kind == "wifi" { "Wi-Fi" } else { "Wired" };
                let addr = addresses(&d.name);
                let addr = if addr.is_empty() {
                    String::new()
                } else {
                    format!(", {addr}")
                };
                (
                    d.name.clone(),
                    format!("{kind} ({}): {}{addr}", d.name, d.state),
                )
            })
            .collect();
        items.push(("nmtui".into(), "Advanced settings (nmtui)".into()));
        items.push(("done".into(), "Done".into()));
        let Some(choice) = ui::menu(
            "Network",
            "Choose a connection to set up. Saved networks connect by themselves.",
            &items,
            Some("done"),
        )?
        else {
            return Ok(());
        };
        let result = match choice.as_str() {
            "done" => return Ok(()),
            "nmtui" => {
                let _ = Command::new("nmtui")
                    .env("NEWT_COLORS", ui::COLORS)
                    .status();
                Ok(())
            }
            name => match devs.iter().find(|d| d.name == name) {
                Some(d) if d.kind == "wifi" => wifi_screen(&d.name),
                Some(d) => ethernet_screen(&d.name),
                None => Ok(()),
            },
        };
        if let Err(e) = result {
            ui::message("Network", &format!("It did not work: {e}"))?;
        }
    }
}

fn ethernet_screen(device: &str) -> Result<()> {
    let Some(ipv4) = ipv4_screen()? else {
        return Ok(());
    };
    ui::working("Network", &format!("Configuring {device}..."));
    configure_ethernet(device, &ipv4)?;
    ui::message(
        "Network",
        &format!("{device} is configured.\n\n{}", status_text()?),
    )
}

fn wifi_screen(device: &str) -> Result<()> {
    ui::working("Wi-Fi", "Looking for networks...");
    let out = output(
        "nmcli",
        &[
            "--terse",
            "--fields",
            "SSID,SIGNAL,SECURITY",
            "device",
            "wifi",
            "list",
            "ifname",
            device,
            "--rescan",
            "yes",
        ],
    )?;
    // (name, signal, secured), strongest first, each name once.
    let mut nets: Vec<(String, u32, bool)> = Vec::new();
    for f in out.lines().map(terse_fields) {
        if f.len() < 3 || f[0].is_empty() || nets.iter().any(|n| n.0 == f[0]) {
            continue;
        }
        let secured = !f[2].is_empty() && f[2] != "--";
        nets.push((f[0].clone(), f[1].parse().unwrap_or(0), secured));
    }
    nets.sort_by_key(|n| std::cmp::Reverse(n.1));
    if nets.is_empty() {
        return ui::message("Wi-Fi", "No Wi-Fi networks found.");
    }
    let items: Vec<(String, String)> = nets
        .iter()
        .map(|(name, signal, secured)| {
            let lock = if *secured { "secured" } else { "open" };
            (name.clone(), format!("{name}  ({signal}%, {lock})"))
        })
        .collect();
    let Some(ssid) = ui::menu("Wi-Fi", "Choose a network:", &items, None)? else {
        return Ok(());
    };
    let secured = nets.iter().any(|n| n.0 == ssid && n.2);
    let password = if secured {
        match ui::password("Wi-Fi", &format!("Password for {ssid}:"))? {
            Some(p) => Some(p),
            None => return Ok(()),
        }
    } else {
        None
    };
    let Some(ipv4) = ipv4_screen()? else {
        return Ok(());
    };
    ui::working("Wi-Fi", &format!("Connecting to {ssid}..."));
    connect_wifi(device, &ssid, password.as_deref(), &ipv4)?;
    ui::message(
        "Wi-Fi",
        &format!("Connected to {ssid}.\n\n{}", status_text()?),
    )
}

/// DHCP or manual; for manual, ask the address, gateway and DNS servers.
fn ipv4_screen() -> Result<Option<Ipv4>> {
    let items = [
        ("dhcp".to_string(), "Automatic (DHCP)".to_string()),
        (
            "manual".to_string(),
            "Manual: address, gateway, DNS".to_string(),
        ),
    ];
    let Some(choice) = ui::menu(
        "Address",
        "How does this connection get its address?",
        &items,
        Some("dhcp"),
    )?
    else {
        return Ok(None);
    };
    if choice == "dhcp" {
        return Ok(Some(Ipv4::Dhcp));
    }
    let mut address = String::new();
    let mut gateway = String::new();
    let mut dns = String::from("1.1.1.1, 9.9.9.9");
    loop {
        let Some(a) = ui::input(
            "Address",
            "Address and prefix (e.g. 192.168.1.20/24):",
            &address,
        )?
        else {
            return Ok(None);
        };
        address = a;
        let Some(g) = ui::input("Address", "Gateway (e.g. 192.168.1.1):", &gateway)? else {
            return Ok(None);
        };
        gateway = g;
        let Some(d) = ui::input("Address", "DNS servers, separated by commas:", &dns)? else {
            return Ok(None);
        };
        dns = d;
        match manual(&address, &gateway, &split_list(&dns)) {
            Ok(ipv4) => return Ok(Some(ipv4)),
            Err(e) => ui::message("Address", &e.to_string())?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse() {
        assert_eq!(
            terse_fields("wlan0:wifi:connected"),
            ["wlan0", "wifi", "connected"]
        );
        assert_eq!(terse_fields(r"My\:Net:80:WPA2"), ["My:Net", "80", "WPA2"]);
    }

    #[test]
    fn addresses_check() {
        assert!(manual("192.168.1.20/24", "192.168.1.1", &["1.1.1.1".into()]).is_ok());
        assert!(manual("192.168.1.20", "192.168.1.1", &[]).is_err());
        assert!(manual("192.168.1.20/33", "192.168.1.1", &[]).is_err());
        assert!(manual("10.0.0.2/8", "nope", &[]).is_err());
        assert_eq!(split_list("1.1.1.1, 9.9.9.9"), ["1.1.1.1", "9.9.9.9"]);
    }
}
