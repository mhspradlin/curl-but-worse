use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::iter::FromIterator;

use tokio::io::AsyncBufReadExt;
use tokio::join;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

#[tokio::main]
pub async fn main() {
    let (mut command_rx, result_tx, session_handle) = create_session();

    let dispatcher = tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            println!("(dispatch) Got command: {:?}", command);
            for url in command.urls {
                let command_id = command.id.clone();
                result_tx.send(CommandResult {
                    command_id,
                    url,
                    output: "What a great result!".into(),
                }).await;
            }
        }
        println!("Done dispatching!");
    });

    join!(
        session_handle,
        dispatcher
    );
    println!("Bye!");
}

fn create_session() -> (Receiver<Command>, Sender<CommandResult>, JoinHandle<()>) {
    let (reader_tx, mut reader_rx) = mpsc::channel(32);


    let reader = std::thread::spawn(move || {
        let mut quit_commands: HashSet<String> = HashSet::new();
        quit_commands.insert("q".into());
        quit_commands.insert("quit".into());
        let mut stdin = std::io::stdin();
        print!("Type URLs: ");
        std::io::stdout().flush().unwrap();
        for line_result in stdin.lock().lines() {
            let line = line_result.unwrap();
            if quit_commands.contains(&line) {
                println!("Got quit command");
                break;
            }
            let mut urls = HashSet::new();
            for url in line.split(" ") {
                urls.insert(url.into());
            }
            reader_tx.blocking_send(Command {
                id: "id123".into(),
                urls
            }).unwrap();
            print!("Type URLs: ");
            std::io::stdout().flush().unwrap();
        }
        println!("Done reading!");
    });


    let (writer_tx, mut writer_rx) = mpsc::channel(32);
    let writer = tokio::spawn(async move {
        while let Some(result) = writer_rx.recv().await {
            println!("Got result: {:?}", result);
        }
        println!("Done writing!");
    });

    (reader_rx, writer_tx, tokio::spawn(async {
        // I think this is in reverse order of actual shutdown, so it's okay if
        // the join() blocks this task since it should already have shut down by the time
        // the writer() closes. Is is good form to wait anyways to make sure we have a clean
        // shutdown?
        writer.await.unwrap();
        reader.join().unwrap();
        ()
    }))
}

#[derive(Debug)]
struct Command {
    id: String,
    urls: HashSet<String>
}

#[derive(Debug)]
struct CommandResult {
    command_id: String,
    url: String,
    output: String
}