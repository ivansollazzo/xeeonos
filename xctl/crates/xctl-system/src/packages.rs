//! `xctl packages`: which packages (and versions) make up this version of
//! XeeonOS. The list is written at build time (mkosi.conf.d/base/mkosi.finalize),
//! because the image has no package manager.

use std::fs;

use crate::util::{Result, err};

const LIST: &str = "/usr/share/xeeonos/packages";

pub fn run(args: &[String]) -> Result<()> {
    let filter = args.first().map(|s| s.to_lowercase());
    let list = fs::read_to_string(LIST).map_err(|e| err(format!("cannot read {LIST}: {e}")))?;
    // Each line is "name version"; with an argument, only names containing it.
    for line in list.lines() {
        let name = line.split_whitespace().next().unwrap_or("");
        if filter.as_deref().is_none_or(|f| name.contains(f)) {
            println!("{line}");
        }
    }
    Ok(())
}
