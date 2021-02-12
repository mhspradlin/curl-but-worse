use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::sync;

use color_eyre::eyre::{eyre, Result, WrapErr};
use cursive::{CbSink, Cursive, CursiveExt, Printer, Vec2};
use cursive::direction::Orientation::Vertical;
use cursive::traits::*;
use cursive::views::{Dialog, EditView, LinearLayout, ListView, TextView};
use futures::{future, StreamExt};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::try_join;

use messages::{Command, CommandResult};

use crate::terminal::TerminalUI;

mod messages;
mod terminal;
mod dispatcher;

#[tokio::main]
pub async fn main() -> Result<()> {
    color_eyre::install()?;

    let terminal_ui: TerminalUI = terminal::create_session();

    let dispatcher: JoinHandle<Result<()>> = tokio::spawn(dispatcher::dispatcher(terminal_ui.command_source, terminal_ui.result_sink));

    future::try_join_all(vec![terminal_ui.join_handle, dispatcher]).await
        .wrap_err("Error waiting on terminal UI or dispatcher")?;

    println!("Bye!");
    Ok(())
}


