use std::collections::HashSet;
use std::io::{BufRead, Write};

use color_eyre::eyre::{eyre, Result, WrapErr};
use tokio::join;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use futures::future;

#[tokio::main]
pub async fn main() -> Result<()> {
    color_eyre::install()?;

    let (command_rx, result_tx, session_handle) = create_session();

    let dispatcher: JoinHandle<Result<()>> = tokio::spawn(dispatcher(command_rx, result_tx));

    let (j0, j1) = join!(
        session_handle,
        dispatcher
    );
    j0??;
    j1??;
    println!("Bye!");
    Ok(())
}

fn create_session() -> (Receiver<Command>, Sender<CommandResult>, JoinHandle<Result<()>>) {
    let quit_commands = create_quit_commands();
    let (reader_tx, reader_rx) = mpsc::channel(32);

    let reader: std::thread::JoinHandle<Result<()>> = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        print!("Type URLs: ");
        std::io::stdout().flush()?;
        for line_result in stdin.lock().lines() {
            let line = line_result.wrap_err("Error when reading line from stdin")?;
            if quit_commands.contains(line.as_str()) {
                println!("Got quit command");
                break;
            }
            let mut urls = HashSet::new();
            for url in line.split(" ") {
                urls.insert(url.into());
            }
            let command = Command {
                urls,
            };
            reader_tx.blocking_send(command).wrap_err("Error sending Command")?;
            print!("Type URLs: ");
            std::io::stdout().flush()?;
        }
        println!("Done reading!");
        Ok(())
    });


    let (writer_tx, mut writer_rx) = mpsc::channel(32);
    let writer = tokio::spawn(async move {
        while let Some(result) = writer_rx.recv().await {
            println!("Got result: {:?}", result);
        }
        println!("Done writing!");
        ()
    });

    (reader_rx, writer_tx, tokio::spawn(async {
        // I think this is in reverse order of actual shutdown, so it's okay if
        // the join() blocks this task since it should already have shut down by the time
        // the writer() closes. Is is good form to wait anyways to make sure we have a clean
        // shutdown?
        writer.await?;
        match reader.join() {
            Err(e) => return Err(eyre!("Shutdown of reader thread failed: {:?}", e)),
            Ok(r) => r?
        };
        Ok(())
    }))
}

fn create_quit_commands() -> HashSet<&'static str> {
    let mut quit_commands = HashSet::new();
    quit_commands.insert("q");
    quit_commands.insert("quit");
    quit_commands
}

async fn dispatcher(mut command_rx: Receiver<Command>, result_tx: Sender<CommandResult>) -> Result<()> {
    while let Some(command) = command_rx.recv().await {
        println!("(dispatch) Got command: {:?}", command);
        let requests = command.urls.iter()
            .map(|url| tokio::spawn(request_url(url.clone(), result_tx.clone())))
            .collect();
        tokio::spawn(run_requests(requests));
    }
    println!("Done dispatching!");
    Ok(())
}

async fn request_url(url: String, result_tx: Sender<CommandResult>) -> Result<()> {
    let response = reqwest::get(&url).await;
    println!("Got response: {:?}", response);
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

async fn run_requests(requests: Vec<JoinHandle<Result<()>>>) {
    future::join_all(requests).await;
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