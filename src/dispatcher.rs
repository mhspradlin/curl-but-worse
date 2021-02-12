use color_eyre::eyre::{Result, WrapErr};

use futures::future;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::messages::{Command, CommandResult};

pub async fn dispatcher(
    mut command_source: Receiver<Command>,
    result_sink: Sender<CommandResult>,
) -> Result<()> {
    // Block is to control lifetime of drain_tx
    let drain_handle = {
        let (drain_tx, drain_rx) = mpsc::channel::<JoinHandle<Result<()>>>(32);
        let drain_handle = tokio::spawn(drain_requests(drain_rx));
        while let Some(command) = command_source.recv().await {
            //println!("(dispatch) Got command: {:?}", command);
            let requests = command
                .urls
                .iter()
                .map(|url| tokio::spawn(request_url(url.clone(), result_sink.clone())))
                .collect();
            let request_handle = tokio::spawn(run_requests(requests));
            drain_tx
                .send(request_handle)
                .await
                .wrap_err("Error sending request to drain")?;
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
            output: format!(
                "{} {}",
                e.status()
                    .map(|status| status.as_str().to_string())
                    .unwrap_or("<none>".to_string()),
                e.to_string()
            ),
        },
    };
    result_tx
        .send(command_result)
        .await
        .wrap_err("Error sending CommandResult")
}

async fn run_requests(requests: Vec<JoinHandle<Result<()>>>) -> Result<()> {
    future::try_join_all(requests).await?;
    Ok(())
}
