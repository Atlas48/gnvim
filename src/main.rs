#![cfg_attr(feature = "unstable", feature(test))]

#[cfg(feature = "libwebkit2gtk")]
#[macro_use]
extern crate lazy_static;
#[cfg(feature = "libwebkit2gtk")]
extern crate ammonia;
#[cfg(feature = "libwebkit2gtk")]
extern crate pulldown_cmark;
extern crate structopt;
#[cfg(feature = "libwebkit2gtk")]
extern crate syntect;

extern crate cairo;
extern crate gdk;
extern crate gdk_pixbuf;
extern crate gio;
extern crate glib;
extern crate gtk;
extern crate log;
extern crate neovim_lib;
extern crate pango;
extern crate pangocairo;
#[cfg(feature = "libwebkit2gtk")]
extern crate webkit2gtk;

use gio::prelude::*;

use neovim_lib::neovim::{Neovim, UiAttachOptions};
use neovim_lib::session::Session as NeovimSession;
use neovim_lib::NeovimApi;

use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;

use structopt::{clap, StructOpt};

include!(concat!(env!("OUT_DIR"), "/gnvim_version.rs"));

mod nvim_bridge;
#[cfg(feature = "libwebkit2gtk")]
mod thread_guard;
mod ui;

fn parse_geometry(input: &str) -> Result<(i32, i32), String> {
    let ret_tuple: Vec<&str> = input.split("x").collect();
    if ret_tuple.len() != 2 {
        Err(String::from("must be of form 'width'x'height'"))
    } else {
        match (ret_tuple[0].parse(), ret_tuple[1].parse()) {
            (Ok(x), Ok(y)) => Ok((x, y)),
            (_, _) => {
                Err(String::from("at least one argument wasn't an integer"))
            }
        }
    }
}

/// Gnvim is a graphical UI for neovim.
#[derive(StructOpt, Debug)]
#[structopt(
    name = "gnvim",
    version = VERSION,
    author = "Ville Hakulinen"
)]
struct Options {
    /// Prints the executed neovim command.
    #[structopt(long = "print-nvim-cmd")]
    print_nvim_cmd: bool,

    /// Path to neovim binary.
    #[structopt(long = "nvim", name = "BIN", default_value = "nvim")]
    nvim_path: String,

    /// Path for gnvim runtime files.
    #[structopt(
        long = "gnvim-rtp",
        default_value = "/usr/local/share/gnvim/runtime",
        env = "GNVIM_RUNTIME_PATH"
    )]
    gnvim_rtp: String,

    /// Files to open.
    #[structopt(value_name = "FILES")]
    open_files: Vec<String>,

    /// Arguments that are passed to nvim.
    #[structopt(value_name = "ARGS", last = true)]
    nvim_args: Vec<String>,

    /// Disables externalized popup menu
    #[structopt(long = "disable-ext-popupmenu")]
    disable_ext_popupmenu: bool,

    /// Disables externalized command line
    #[structopt(long = "disable-ext-cmdline")]
    disable_ext_cmdline: bool,

    /// Disables externalized tab line
    #[structopt(long = "disable-ext-tabline")]
    disable_ext_tabline: bool,

    /// Geometry of the window in widthxheight form
    #[structopt(long = "geometry", parse(try_from_str = parse_geometry), default_value = "1280x720")]
    geometry: (i32, i32),
}

fn build(app: &gtk::Application, opts: &Options) {
    let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

    let bridge = nvim_bridge::NvimBridge::new(tx);

    let mut cmd = Command::new(&opts.nvim_path);
    cmd.arg("--embed")
        .arg("--cmd")
        .arg("let g:gnvim=1")
        .arg("--cmd")
        .arg("set termguicolors")
        .arg("--cmd")
        .arg(format!("let &rtp.=',{}'", opts.gnvim_rtp));

    // Pass arguments from cli to nvim.
    for arg in opts.nvim_args.iter() {
        cmd.arg(arg);
    }

    // Open files "normally" through nvim.
    for file in opts.open_files.iter() {
        cmd.arg(file);
    }

    // Print the nvim cmd which is executed if asked.
    if opts.print_nvim_cmd {
        println!("nvim cmd: {:?}", cmd);
    }

    let mut session = NeovimSession::new_child_cmd(&mut cmd).unwrap();
    session.start_event_loop_handler(bridge);

    let mut nvim = Neovim::new(session);
    nvim.subscribe("Gnvim")
        .expect("Failed to subscribe to 'Gnvim' events");

    let api_info = nvim.get_api_info().expect("Failed to get API info");
    nvim.set_var("gnvim_channel_id", api_info[0].clone())
        .expect("Failed to set g:gnvim_channel_id");

    let mut ui_opts = UiAttachOptions::new();
    ui_opts.set_rgb(true);
    ui_opts.set_linegrid_external(true);
    ui_opts.set_popupmenu_external(!opts.disable_ext_popupmenu);
    ui_opts.set_tabline_external(!opts.disable_ext_tabline);
    ui_opts.set_cmdline_external(!opts.disable_ext_cmdline);

    ui_opts.set_wildmenu_external(true);
    nvim.ui_attach(80, 30, &ui_opts)
        .expect("Failed to attach UI");

    let ui = ui::UI::init(app, rx, opts.geometry, Rc::new(RefCell::new(nvim)));
    ui.start();
}

fn main() {
    env_logger::init();

    let opts = Options::clap();
    let opts = Options::from_clap(&opts.get_matches_safe().unwrap_or_else(
        |mut err| {
            if let clap::ErrorKind::UnknownArgument = err.kind {
                // Arg likely passed for nvim, notify user of how to pass args to nvim.
                err.message = format!(
                    "{}\n\nIf this is an argument for nvim, try moving \
                     it after a -- separator.",
                    err.message
                );
                err.exit();
            } else {
                err.exit()
            }
        },
    ));

    let mut flags = gio::ApplicationFlags::empty();
    flags.insert(gio::ApplicationFlags::NON_UNIQUE);
    flags.insert(gio::ApplicationFlags::HANDLES_OPEN);
    let app = gtk::Application::new(Some("com.github.vhakulinen.gnvim"), flags)
        .unwrap();

    gdk::set_program_class("GNvim");
    glib::set_application_name("GNvim");
    gtk::Window::set_default_icon_name("gnvim");

    app.connect_activate(move |app| {
        build(app, &opts);
    });

    app.run(&vec![]);
}
