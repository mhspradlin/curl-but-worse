use color_eyre::eyre::{Result, WrapErr};

use futures::future;

use tokio::task::JoinHandle;

use crate::terminal::TerminalUI;

mod dispatcher;
mod messages;
mod terminal;

#[tokio::main]
pub async fn main() -> Result<()> {
    color_eyre::install()?;

    let terminal_ui: TerminalUI = terminal::create_session();

    let dispatcher: JoinHandle<Result<()>> = tokio::spawn(dispatcher::dispatcher(
        terminal_ui.command_source,
        terminal_ui.result_sink,
    ));

    future::try_join_all(vec![terminal_ui.join_handle, dispatcher])
        .await
        .wrap_err("Error waiting on terminal UI or dispatcher")?;

    println!("Bye!");
    Ok(())
}
