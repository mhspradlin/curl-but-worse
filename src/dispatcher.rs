use color_eyre::eyre::{Result, WrapErr};

use futures::future;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::messages::{Command, CommandResult};
use reqwest::Client;
use std::time::Duration;

pub async fn dispatcher(
    mut command_source: Receiver<Command>,
    result_sink: Sender<CommandResult>,
) -> Result<()> {
    // Block is to control lifetime of drain_tx
    let drain_handle = {
        let (drain_tx, drain_rx) = mpsc::channel::<JoinHandle<Result<()>>>(32);
        let drain_handle = tokio::spawn(drain_requests(drain_rx));
        let http_client: Client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .wrap_err("Unable to build HTTP client")?;
        while let Some(command) = command_source.recv().await {
            let requests = command
                .urls
                .iter()
                .map(|url| tokio::spawn(request_url(http_client.clone(), url.clone(), result_sink.clone())))
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

async fn request_url(http_client: Client, url: String, result_tx: Sender<CommandResult>) -> Result<()> {
    let request = http_client.get(&url)
        .build()
        .wrap_err("Unable to build GET request")?;
    let response = http_client.execute(request).await;
    let command_result = match response {
        Ok(result) => {
            let status = result.status().as_str().to_string();
            let mut start_of_response: String = result
                .text()
                .await
                .unwrap_or("<error getting body>".to_string())
                .chars()
                .take(60)
                .collect();
            if start_of_response.len() == 60 {
                start_of_response.push_str("...");
            }
            CommandResult {
                url,
                output: format!("{} {}", status, start_of_response),
            }
        }
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
