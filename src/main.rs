use std::collections::HashSet;
use std::io::{BufRead, Write, Error};

use tokio::io::{ErrorKind};
use tokio::join;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

#[tokio::main]
pub async fn main() -> Result<(), Error> {
    let (mut command_rx, result_tx, session_handle) = create_session();

    let dispatcher: JoinHandle<Result<(), Error>> = tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            println!("(dispatch) Got command: {:?}", command);
            for url in command.urls {
                let command_id = command.id.clone();
                result_tx.send(CommandResult {
                    command_id,
                    url,
                    output: "What a great result!".into(),
                }).await.map_err(|e| Error::new(ErrorKind::Other, format!("Error sending CommandResult: {:?}", e)))?;
            }
        }
        println!("Done dispatching!");
        Ok(())
    });

    let (j0, j1) = join!(
        session_handle,
        dispatcher
    );
    j0??;
    j1??;
    println!("Bye!");
    Ok(())
}

fn create_session() -> (Receiver<Command>, Sender<CommandResult>, JoinHandle<Result<(), Error>>) {
    let quit_commands = create_quit_commands();
    let (reader_tx, reader_rx) = mpsc::channel(32);

    let reader: std::thread::JoinHandle<Result<(), Error>> = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        print!("Type URLs: ");
        std::io::stdout().flush()?;
        for line_result in stdin.lock().lines() {
            let line = line_result
                .map_err(|e| Error::new(ErrorKind::Other, format!("Error when reading line from stdin: {:?}", e)))?;
            if quit_commands.contains(line.as_str()) {
                println!("Got quit command");
                break;
            }
            let mut urls = HashSet::new();
            for url in line.split(" ") {
                urls.insert(url.into());
            }
            reader_tx.blocking_send(Command {
                id: "id123".into(),
                urls,
            }).map_err(|e| Error::new(ErrorKind::Other, format!("Error sending Command: {:?}", e)))?;
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
            Err(e) =>
                return Err(Error::new(ErrorKind::Other,
                                      format!("Shutdown of reader thread failed: {:?}", e))),
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


#[derive(Debug)]
struct Command {
    id: String,
    urls: HashSet<String>,
}

#[derive(Debug)]
struct CommandResult {
    command_id: String,
    url: String,
    output: String,
}