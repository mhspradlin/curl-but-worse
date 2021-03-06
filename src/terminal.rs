use std::collections::HashSet;

use color_eyre::eyre::{eyre, WrapErr};
use color_eyre::Result;
use cursive::{CbSink, Cursive, CursiveExt};
use cursive::direction::Orientation::Vertical;
use cursive::traits::{Nameable, Resizable, Scrollable};
use cursive::views::{Dialog, EditView, LinearLayout, ListView, TextView};
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::messages::{Command, CommandResult};
use cursive::view::ScrollStrategy;
use cursive::event::EventResult;

pub struct TerminalUI {
    pub command_source: Receiver<Command>,
    pub result_sink: Sender<CommandResult>,
    pub join_handle: JoinHandle<Result<()>>,
}

pub fn create_session() -> TerminalUI {
    let _quit_commands = create_quit_commands();
    let (reader_tx, reader_rx) = mpsc::channel(32);
    let (writer_tx, writer_rx) = mpsc::channel(32);
    let (cb_sink_tx, cb_sink_rx) = oneshot::channel();

    let reader: std::thread::JoinHandle<Result<()>> = std::thread::spawn(move || {
        let mut siv = Cursive::new();
        siv.add_global_callback('q', |s| s.quit());

        let submit_reader_tx = reader_tx.clone();

        let results_list = ListView::new().with_name("results-list")
            .scrollable()
            .scroll_strategy(ScrollStrategy::StickToBottom)
            .on_scroll_inner(|scroll_view, _rect| {
                if scroll_view.is_at_bottom() {
                    scroll_view.set_scroll_strategy(ScrollStrategy::StickToBottom);
                    EventResult::Consumed(None)
                } else {
                    EventResult::Ignored
                }
            });

        // Create a dialog with an edit text and a button.
        // The user can either hit the <Ok> button,
        // or press Enter on the edit text.
        siv.add_layer(
            LinearLayout::new(Vertical)
                .child(
                    Dialog::new()
                        .title("Results")
                        .padding_lrtb(1, 1, 1, 0)
                        .content(results_list)
                        .full_height(),
                )
                .child(
                    Dialog::new()
                        .title("Enter URLs")
                        .padding_lrtb(1, 1, 1, 1)
                        .content(EditView::new().on_submit(move |cursive, line| {
                            send_command(cursive, submit_reader_tx.clone(), line)
                        })),
                )
                .full_screen()
        );

        cb_sink_tx
            .send(siv.cb_sink().clone())
            .map_err(|e| eyre!("Error sending cb_sink on initialization: {:?}", e))?;

        siv.run();
        Ok(())
    });

    let writer = tokio::spawn(display_results(cb_sink_rx, writer_rx));

    let join_handle = tokio::spawn(async {
        // I think it's okay if the join() blocks this task since it should
        // always be the first thing that shuts down in the application. In theory we shouldn't
        // block though since it might permanently block a single-threaded executor. Maybe one
        // of the threads in our executor is permanently blocked because of this? Probably good to
        // avoid this kind of infinite blocking in a more serious application.
        match reader.join() {
            Err(e) => return Err(eyre!("Shutdown of reader thread failed: {:?}", e)),
            Ok(r) => r?,
        };
        writer.await??;
        Ok(())
    });

    TerminalUI {
        command_source: reader_rx,
        result_sink: writer_tx,
        join_handle,
    }
}

async fn display_results(
    cb_sink_rx: oneshot::Receiver<CbSink>,
    mut result_rx: Receiver<CommandResult>,
) -> Result<()> {
    let cb_sink: CbSink = cb_sink_rx.await.wrap_err("Error getting cb_sink")?;
    while let Some(result) = result_rx.recv().await {
        cb_sink
            .send(Box::new(move |siv: &mut Cursive| {
                siv.call_on_name("results-list", |view: &mut ListView| {
                    add_result(view, result);
                });
            }))
            .map_err(|e| eyre!("Error sending UI update to cb_sink: {:?}", e))?;
    }
    Ok(())
}

fn add_result(view: &mut ListView, result: CommandResult) {
    view.add_child(result.url.as_str(), TextView::new(result.output));
}

fn send_command(s: &mut Cursive, reader_tx: Sender<Command>, line: &str) {
    if line.is_empty() {
        // Try again as many times as we need!
        s.add_layer(Dialog::info("Please enter URLs"));
    } else {
        let mut urls = HashSet::new();
        for url in line.split(" ") {
            urls.insert(url.into());
        }
        let command = Command { urls };
        reader_tx
            .blocking_send(command)
            .wrap_err("Error sending Command")
            .unwrap();
    }
}

fn create_quit_commands() -> HashSet<&'static str> {
    let mut quit_commands = HashSet::new();
    quit_commands.insert("q");
    quit_commands.insert("quit");
    quit_commands
}
