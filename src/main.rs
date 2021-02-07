use std::collections::HashSet;
use std::io::{BufRead, Write};

use color_eyre::eyre::{eyre, Result, WrapErr};
use cursive::{Cursive, CursiveExt, Vec2, Printer, CbSink};
use cursive::views::{TextView, Dialog, EditView, LinearLayout, ListView};
use cursive::traits::*;
use std::sync;
use futures::future;
use tokio::try_join;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use cursive::direction::Orientation::Vertical;

#[tokio::main]
pub async fn main() -> Result<()> {
    color_eyre::install()?;

    let (command_rx, result_tx, session_handle) = create_session().await;

    let dispatcher: JoinHandle<Result<()>> = tokio::spawn(dispatcher(command_rx, result_tx));

    let (h0, h1) = try_join!(session_handle, dispatcher)?;
    h0?;
    h1?;
    println!("Bye!");
    Ok(())
}

async fn create_session() -> (Receiver<Command>, Sender<CommandResult>, JoinHandle<Result<()>>) {
    let quit_commands = create_quit_commands();
    let (reader_tx, reader_rx) = mpsc::channel(32);
    let (writer_tx, mut writer_rx) = mpsc::channel(32);
    let (cb_sink_tx, cb_sink_rx) = tokio::sync::oneshot::channel();


    let reader: std::thread::JoinHandle<Result<()>> = std::thread::spawn(move || {
        let mut siv = Cursive::new();
        siv.add_global_callback('q', |s| s.quit());

        let submit_reader_tx = reader_tx.clone();

        let results_list = ListView::new().with_name("results-list");

        // Create a dialog with an edit text and a button.
        // The user can either hit the <Ok> button,
        // or press Enter on the edit text.
        siv.add_layer(
            LinearLayout::new(Vertical)
                .child(
                    Dialog::new()
                        .title("Results")
                        .padding_lrtb(1, 1, 1, 1)
                        .content(results_list))
                .child(
                    Dialog::new()
                        .title("Enter URLs")
                        // Padding is (left, right, top, bottom)
                        .padding_lrtb(1, 1, 1, 0)
                        .content(
                            EditView::new()
                                // Call `show_popup` when the user presses `Enter`
                                .on_submit(move |cursive, line| send_command(cursive, submit_reader_tx.clone(), line))
                                // Give the `EditView` a name so we can refer to it later.
                                .with_name("name")
                                // Wrap this in a `ResizedView` with a fixed width.
                                // Do this _after_ `with_name` or the name will point to the
                                // `ResizedView` instead of `EditView`!
                                .fixed_width(50),
                        ))
        );

        cb_sink_tx.send(siv.cb_sink().clone());

        siv.run();
        Ok(())
    });

    let writer = tokio::spawn(display_results(cb_sink_rx.await.expect("Error getting cb_sink"), writer_rx));

    (reader_rx, writer_tx, tokio::spawn(async {
        // I think this is in reverse order of actual shutdown, so it's okay if
        // the join() blocks this task since it should already have shut down by the time
        // the writer() closes. Is is good form to wait anyways to make sure we have a clean
        // shutdown?
        //writer.await?;
        match reader.join() {
            Err(e) => return Err(eyre!("Shutdown of reader thread failed: {:?}", e)),
            Ok(r) => r?
        };
        Ok(())
    }))
}

async fn display_results(cb_sink: CbSink, mut result_rx: Receiver<CommandResult>) -> Result<()> {
    while let Some(result) = result_rx.recv().await {
        cb_sink.send(Box::new(move |siv: &mut Cursive| {
            siv.call_on_name("results-list", |view: &mut ListView| {
                add_result(view, result);
            });
        }));
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
        let command = Command {
            urls,
        };
        reader_tx.blocking_send(command).wrap_err("Error sending Command").unwrap();
    }
}

fn create_quit_commands() -> HashSet<&'static str> {
    let mut quit_commands = HashSet::new();
    quit_commands.insert("q");
    quit_commands.insert("quit");
    quit_commands
}

async fn dispatcher(mut command_rx: Receiver<Command>, result_tx: Sender<CommandResult>) -> Result<()> {
    // Block is to control lifetime of drain_tx
    let drain_handle = {
        let (drain_tx, drain_rx) = mpsc::channel::<JoinHandle<Result<()>>>(32);
        let drain_handle = tokio::spawn(drain_requests(drain_rx));
        while let Some(command) = command_rx.recv().await {
            //println!("(dispatch) Got command: {:?}", command);
            let requests = command.urls.iter()
                .map(|url| tokio::spawn(request_url(url.clone(), result_tx.clone())))
                .collect();
            let request_handle = tokio::spawn(run_requests(requests));
            drain_tx.send(request_handle).await.wrap_err("Error sending request to drain")?;
        }
        drain_handle
    };
    println!("Done dispatching!");
    drain_handle.await.wrap_err("Error draining requests")??;
    println!("Done draining!");
    Ok(())
}

async fn drain_requests(mut requests_rx: Receiver<JoinHandle<Result<()>>>) -> Result<()> {
    while let Some(join_handle) = requests_rx.recv().await {
        join_handle.await.wrap_err("Request returned error")??;
    }
    println!("Done waiting on all requests!");
    Ok(())
}

async fn request_url(url: String, result_tx: Sender<CommandResult>) -> Result<()> {
    let response = reqwest::get(&url).await;
    //println!("Got response: {:?}", response);
    let command_result = match response {
        Ok(result) => CommandResult {
            url,
            output: format!("{}", result.status().as_str()),
        },
        Err(e) => CommandResult {
            url,
            output: format!("{} {}", e.status().map(|status| status.as_str().to_string())
                .unwrap_or("<none>".to_string()), e.to_string()),
        }
    };
    result_tx.send(command_result).await.wrap_err("Error sending CommandResult")
}

async fn run_requests(requests: Vec<JoinHandle<Result<()>>>) -> Result<()> {
    future::try_join_all(requests).await?;
    Ok(())
}

#[derive(Debug)]
struct Command {
    urls: HashSet<String>,
}

#[derive(Debug)]
struct CommandResult {
    url: String,
    output: String,
}