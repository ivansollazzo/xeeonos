//! build, and the commands the system uses: sync (the monitor's unit),
//! dialog (the dialog window), thumbnail (the icon Files shows for a .xapp).

use std::path::PathBuf;

use xapp_core::util::{Result, err, have, run as run_program};
use xapp_format::Container;

/// `xapp build <recipe> [--key <file>] [--output <file>] [--work <dir>]
/// [--no-install]`.
pub fn build(args: &[String]) -> Result<u8> {
    let mut o = xapp_build::Options {
        recipe: PathBuf::new(),
        key: None,
        output: None,
        work: None,
        root: None,
    };
    let mut install = true;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .map(PathBuf::from)
                .ok_or_else(|| err(format!("{a} needs a value")))
        };
        match a.as_str() {
            "--key" => o.key = Some(value()?),
            "--output" | "-o" => o.output = Some(value()?),
            "--work" => o.work = Some(value()?),
            "--root" => o.root = Some(value()?),
            "--no-install" => install = false,
            other if other.starts_with('-') => return Err(err(format!("unknown option {other}"))),
            other => o.recipe = PathBuf::from(other),
        }
    }
    if o.recipe.as_os_str().is_empty() {
        return Err(err(
            "usage: xapp build <recipe dir> [--key <file>] [--output <file>] [--no-install]",
        ));
    }
    let out = xapp_build::build(&o)?;
    if install {
        super::manage::install(&[out.display().to_string()])?;
    }
    Ok(0)
}

/// `xapp sync [--quiet]`: run by xapp-sync.service (the monitor).
pub fn sync(args: &[String]) -> Result<u8> {
    let quiet = args.iter().any(|a| a == "--quiet" || a == "-q");
    let r = xapp_desktop::sync()?;
    if !quiet {
        for id in &r.registered {
            println!("Registered: {id}");
        }
        for id in &r.removed {
            println!("Removed from the menu: {id}");
        }
    }
    for (f, why) in &r.skipped {
        println!("Left out: {} ({why})", f.display());
    }
    Ok(0)
}

/// `xapp dialog <file>`: run in the dialog window (xapp_core::ui).
pub fn dialog(args: &[String]) -> Result<u8> {
    let file = args
        .first()
        .ok_or_else(|| err("usage: xapp dialog <file>"))?;
    xapp_core::ui::answer_file(std::path::Path::new(file))?;
    Ok(0)
}

/// `xapp thumbnail <file> <output.png> [size]`: the thumbnailer of Files
/// (/usr/share/thumbnailers/xapp.thumbnailer): the application's icon.
pub fn thumbnail(args: &[String]) -> Result<u8> {
    let (Some(input), Some(output)) = (args.first(), args.get(1)) else {
        return Err(err("usage: xapp thumbnail <file> <output.png> [size]"));
    };
    let c = Container::open(std::path::Path::new(input))?;
    if c.manifest.icon_type == "png" {
        std::fs::write(output, &c.icon)?;
        return Ok(0);
    }
    if !have("rsvg-convert") {
        return Err(err("no rsvg-convert to draw an SVG icon"));
    }
    let svg = std::env::temp_dir().join(format!("xapp-icon-{}.svg", std::process::id()));
    std::fs::write(&svg, &c.icon)?;
    let size = args.get(2).cloned().unwrap_or_else(|| "256".into());
    let result = run_program(
        "rsvg-convert",
        &[
            "-w",
            &size,
            "-h",
            &size,
            "-o",
            output,
            &svg.to_string_lossy(),
        ],
    );
    let _ = std::fs::remove_file(&svg);
    result.map(|_| 0)
}
