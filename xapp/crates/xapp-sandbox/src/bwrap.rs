//! The sandbox: bubblewrap's command line.
//!
//! bubblewrap (bwrap, the same tool Flatpak uses underneath) starts the
//! application in new namespaces (user, mount, process ids, IPC, and network
//! when not allowed), on an empty root it fills piece by piece:
//!
//!   /usr, /opt, ... (and /bin, /lib links)   from the application's image,
//!                                            read-only
//!   /etc     the image's, plus what the system lends: hosts, time zone,
//!            DNS, certificates, a passwd with only the user
//!   /proc /dev /sys(read-only, devices)  /tmp /var /run (empty)
//!   $HOME    the application's data, ~/.var/xapps/<id>, read-write
//!   $HOME/<folder>   folders the user gave (Documents, ...)
//!   /run/user/<uid>  Wayland, PipeWire (audio), the filtered session bus
//!   /run/host        fonts, icons, themes, lent graphics drivers
//!   /run/xapp/docs   files the application was asked to open
//!
//! Nothing else of the system or of the home is there.

use std::fs;
use std::path::{Path, PathBuf};

/// The arguments being built.
#[derive(Default)]
pub struct Args(pub Vec<String>);

impl Args {
    fn push(&mut self, items: &[&str]) {
        self.0.extend(items.iter().map(|s| s.to_string()));
    }
    pub fn ro_bind(&mut self, src: &Path, dst: &str) {
        self.push(&["--ro-bind", &src.to_string_lossy(), dst]);
    }
    pub fn ro_bind_try(&mut self, src: &Path, dst: &str) {
        self.push(&["--ro-bind-try", &src.to_string_lossy(), dst]);
    }
    pub fn bind(&mut self, src: &Path, dst: &str) {
        self.push(&["--bind", &src.to_string_lossy(), dst]);
    }
    pub fn dev_bind_try(&mut self, src: &Path, dst: &str) {
        self.push(&["--dev-bind-try", &src.to_string_lossy(), dst]);
    }
    pub fn symlink(&mut self, target: &Path, link: &str) {
        self.push(&["--symlink", &target.to_string_lossy(), link]);
    }
    pub fn tmpfs(&mut self, dst: &str) {
        self.push(&["--tmpfs", dst]);
    }
    pub fn dir(&mut self, dst: &str) {
        self.push(&["--dir", dst]);
    }
    pub fn setenv(&mut self, key: &str, value: &str) {
        self.push(&["--setenv", key, value]);
    }
}

/// Everything the command line depends on.
pub struct Context {
    /// The mounted image.
    pub image: PathBuf,
    /// The running instance's own files (passwd, fonts.conf, bus socket).
    pub instance: PathBuf,
    pub app_id: String,
    pub uid: u32,
    pub home: PathBuf,
    /// The application's data, bound as its home.
    pub data: PathBuf,
    /// The session's $XDG_RUNTIME_DIR on the system.
    pub runtime_dir: PathBuf,
    pub network: bool,
    pub gpu: bool,
    pub audio: bool,
    pub camera: bool,
    pub x11: bool,
    /// Folders given (their real paths, bound at the same place).
    pub folders: Vec<PathBuf>,
    /// The filtered session bus, if any.
    pub bus: Option<PathBuf>,
    /// Lent graphics drivers: binds and environment.
    pub gpu_binds: Vec<(PathBuf, String)>,
    pub env: Vec<(String, String)>,
    /// Files to open: (path on the system, path in the sandbox).
    pub docs: Vec<(PathBuf, String)>,
}

/// Top-level entries of the image that are never taken from it.
const SKIP_TOP: &[&str] = &[
    "xapp",
    "etc",
    "proc",
    "sys",
    "dev",
    "run",
    "tmp",
    "var",
    "home",
    "root",
    "mnt",
    "media",
    "srv",
    "boot",
    "lost+found",
];
/// Entries of the image's /etc replaced by the system's (or made here).
const SKIP_ETC: &[&str] = &[
    "passwd",
    "group",
    "shadow",
    "gshadow",
    "hosts",
    "hostname",
    "machine-id",
    "localtime",
    "resolv.conf",
    "ssl",
    "ca-certificates",
    "pki",
    "mtab",
];

/// Lay the entries of `dir` (a directory of the image) at `dest`: links as
/// links, the rest bound read-only.
fn lay(a: &mut Args, dir: &Path, dest: &str, skip: &[&str]) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        if skip.contains(&name.as_str()) {
            continue;
        }
        let target = format!("{dest}/{name}");
        match fs::read_link(e.path()) {
            Ok(link) => a.symlink(&link, &target),
            Err(_) => a.ro_bind(&e.path(), &target),
        }
    }
}

/// The bubblewrap arguments (everything before the command).
pub fn args(c: &Context) -> Args {
    let mut a = Args::default();
    let run_user = format!("/run/user/{}", c.uid);
    let home = c.home.to_string_lossy().into_owned();
    a.push(&[
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--die-with-parent",
        "--cap-drop",
        "ALL",
    ]);
    if !c.network {
        a.push(&["--unshare-net"]);
    }

    // The application's own system.
    lay(&mut a, &c.image, "", SKIP_TOP);
    a.tmpfs("/etc");
    lay(&mut a, &c.image.join("etc"), "/etc", SKIP_ETC);
    a.ro_bind(&c.instance.join("passwd"), "/etc/passwd");
    a.ro_bind(&c.instance.join("group"), "/etc/group");
    a.symlink(Path::new("../proc/self/mounts"), "/etc/mtab");

    a.push(&["--proc", "/proc", "--dev", "/dev"]);
    for d in [
        "/sys/block",
        "/sys/bus",
        "/sys/class",
        "/sys/dev",
        "/sys/devices",
    ] {
        a.ro_bind_try(Path::new(d), d);
    }
    a.tmpfs("/tmp");
    a.tmpfs("/var");
    a.dir("/var/tmp");
    a.symlink(Path::new("../run"), "/var/run");
    a.tmpfs("/run");
    a.push(&["--perms", "0700", "--dir", &run_user]);

    // What the system lends.
    for f in ["/etc/hosts", "/etc/hostname", "/etc/machine-id"] {
        a.ro_bind_try(Path::new(f), f);
    }
    if let Ok(tz) = fs::canonicalize("/etc/localtime") {
        a.ro_bind(&tz, "/etc/localtime");
    }
    for d in ["/etc/ssl", "/etc/ca-certificates", "/etc/pki"] {
        a.ro_bind_try(Path::new(d), d);
    }
    if c.network {
        // DNS: the system's resolv.conf is usually a link into
        // /run/systemd/resolve or /run/NetworkManager, which change
        // their files in place: lend the directory, keep the link.
        match fs::read_link("/etc/resolv.conf") {
            Ok(target) if target.is_absolute() || target.starts_with("..") => {
                let abs = if target.is_absolute() {
                    target.clone()
                } else {
                    Path::new("/etc").join(&target)
                };
                let abs = fs::canonicalize(&abs).unwrap_or(abs);
                if let Some(dir) = abs.parent() {
                    a.ro_bind_try(dir, &dir.to_string_lossy());
                }
                a.symlink(&abs, "/etc/resolv.conf");
            }
            _ => a.ro_bind_try(Path::new("/etc/resolv.conf"), "/etc/resolv.conf"),
        }
    }

    // Home: the application's data; then the folders given.
    a.bind(&c.data, &home);
    for f in &c.folders {
        a.bind(f, &f.to_string_lossy());
    }
    for (src, dst) in &c.docs {
        a.bind(src, dst);
    }

    // Session sockets.
    if let Ok(w) = std::env::var("WAYLAND_DISPLAY") {
        let src = if w.starts_with('/') {
            PathBuf::from(&w)
        } else {
            c.runtime_dir.join(&w)
        };
        a.ro_bind_try(&src, &format!("{run_user}/wayland-0"));
        a.setenv("WAYLAND_DISPLAY", "wayland-0");
        a.setenv("XDG_SESSION_TYPE", "wayland");
    }
    if let Some(bus) = &c.bus {
        a.ro_bind(bus, &format!("{run_user}/bus"));
        a.setenv(
            "DBUS_SESSION_BUS_ADDRESS",
            &format!("unix:path={run_user}/bus"),
        );
    }
    if c.audio || c.camera {
        a.ro_bind_try(
            &c.runtime_dir.join("pipewire-0"),
            &format!("{run_user}/pipewire-0"),
        );
    }
    if c.audio {
        a.ro_bind_try(
            &c.runtime_dir.join("pulse/native"),
            &format!("{run_user}/pulse/native"),
        );
        a.setenv("PULSE_SERVER", &format!("unix:{run_user}/pulse/native"));
    }
    if c.camera
        && let Ok(entries) = fs::read_dir("/dev")
    {
        let mut devs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().unwrap_or_default().to_string_lossy();
                n.starts_with("video") || n.starts_with("media")
            })
            .collect();
        devs.sort();
        for d in devs {
            a.dev_bind_try(&d, &d.to_string_lossy());
        }
    }
    if c.x11
        && let Ok(display) = std::env::var("DISPLAY")
        && let Some(n) = display
            .strip_prefix(':')
            .map(|d| d.split('.').next().unwrap_or("0").to_string())
    {
        let socket = format!("/tmp/.X11-unix/X{n}");
        a.ro_bind_try(Path::new(&socket), &socket);
        a.setenv("DISPLAY", &format!(":{n}"));
        if let Ok(auth) = std::env::var("XAUTHORITY") {
            a.ro_bind_try(Path::new(&auth), &format!("{run_user}/Xauthority"));
            a.setenv("XAUTHORITY", &format!("{run_user}/Xauthority"));
        }
    }

    // Graphics.
    if c.gpu {
        a.dev_bind_try(Path::new("/dev/dri"), "/dev/dri");
        for (src, dst) in &c.gpu_binds {
            a.ro_bind(src, dst);
        }
    }

    // Fonts, icons, themes, languages of the system.
    a.ro_bind_try(Path::new("/usr/share/fonts"), "/run/host/fonts");
    a.ro_bind_try(&c.home.join(".local/share/fonts"), "/run/host/user-fonts");
    a.ro_bind(&c.instance.join("fonts.conf"), "/run/host/fonts.conf");
    a.ro_bind_try(Path::new("/usr/share/icons"), "/run/host/share/icons");
    a.ro_bind_try(Path::new("/usr/share/themes"), "/run/host/share/themes");
    if c.image.join("usr/lib/locale").is_dir() {
        a.ro_bind_try(Path::new("/usr/lib/locale"), "/usr/lib/locale");
    }

    // Environment: only what is listed here.
    a.push(&["--clearenv"]);
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let mut env: Vec<(String, String)> = vec![
        ("PATH".into(), "/usr/local/bin:/usr/bin".into()),
        ("HOME".into(), home.clone()),
        ("USER".into(), user.clone()),
        ("LOGNAME".into(), user),
        ("SHELL".into(), "/bin/sh".into()),
        ("XDG_RUNTIME_DIR".into(), run_user.clone()),
        (
            "XDG_DATA_DIRS".into(),
            "/usr/local/share:/usr/share:/run/host/share".into(),
        ),
        ("FONTCONFIG_FILE".into(), "/run/host/fonts.conf".into()),
        ("XAPP_ID".into(), c.app_id.clone()),
        // No accessibility bus, no GNOME virtual file systems: both live
        // outside the sandbox.
        ("NO_AT_BRIDGE".into(), "1".into()),
        ("GTK_A11Y".into(), "none".into()),
        ("GIO_USE_VFS".into(), "local".into()),
    ];
    for (k, v) in std::env::vars() {
        let pass = k == "LANG"
            || k == "LANGUAGE"
            || k.starts_with("LC_")
            || k == "TERM"
            || k == "COLORTERM"
            || k == "XDG_CURRENT_DESKTOP"
            || k == "XDG_SESSION_DESKTOP"
            || k == "TZ";
        if pass {
            env.push((k, v));
        }
    }
    env.extend(c.env.iter().cloned());
    for (k, v) in &env {
        a.setenv(k, v);
    }
    a.push(&["--chdir", &home]);
    a
}

/// The instance's own files: passwd and group with only the user, and a
/// fontconfig file that adds the system's fonts to the application's.
pub fn write_instance_files(
    instance: &Path,
    uid: u32,
    gid: u32,
    home: &Path,
) -> std::io::Result<()> {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    fs::write(
        instance.join("passwd"),
        format!(
            "root:x:0:0:root:/root:/usr/bin/nologin\n{user}:x:{uid}:{gid}::{}:/bin/sh\nnobody:x:65534:65534:Nobody:/:/usr/bin/nologin\n",
            home.display()
        ),
    )?;
    fs::write(
        instance.join("group"),
        format!("root:x:0:\n{user}:x:{gid}:\nnobody:x:65534:\n"),
    )?;
    fs::write(
        instance.join("fonts.conf"),
        format!(
            "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n<fontconfig>\n  \
             <include ignore_missing=\"yes\">/etc/fonts/fonts.conf</include>\n  \
             <dir>/run/host/fonts</dir>\n  <dir>/run/host/user-fonts</dir>\n  \
             <cachedir>{}/.cache/fontconfig</cachedir>\n</fontconfig>\n",
            home.display()
        ),
    )?;
    Ok(())
}
