//! PIC-Killer —— 照片元数据瑞士军刀。
//!
//! 无损批量修改照片 EXIF：拍摄时间、作者版权、相机镜头、GPS、方向，以及查看与清除。
//! 只改写元数据段，不重新编码图像，像素数据完全无损。

mod cli;
mod commands;
mod exif;
mod gpx;
mod iptc;
mod namedate;
mod scan;
mod timeop;
mod whereexpr;
mod xmp;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

fn main() {
    if let Err(e) = run() {
        eprintln!("错误：{e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let failures = match cli.command {
        Command::Time(args) => commands::time(args)?,
        Command::Show(args) => commands::show(args)?,
        Command::Set(args) => commands::set(args)?,
        Command::Gps(args) => commands::gps(args)?,
        Command::Strip(args) => commands::strip(args)?,
        Command::Rotate(args) => commands::rotate(args)?,
        Command::Copy(args) => commands::copy(args)?,
        Command::Rename(args) => commands::rename(args)?,
        Command::Xmp(args) => commands::xmp(args)?,
        Command::Iptc(args) => commands::iptc(args)?,
        Command::Restore(args) => commands::restore(args)?,
        Command::Geotag(args) => commands::geotag(args)?,
        Command::Apply(args) => commands::apply(args)?,
        Command::Report(args) => commands::report(args)?,
        Command::Completions(args) => commands::completions(args)?,
    };

    if failures > 0 {
        std::process::exit(2);
    }
    Ok(())
}
