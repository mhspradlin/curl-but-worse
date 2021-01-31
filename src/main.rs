use tokio::sync::mpsc;
use tokio::io;
use tokio::io::AsyncBufReadExt;

#[tokio::main]
pub async fn main() {
    let session_task = tokio::spawn(session());

    session_task.await.unwrap();
    ()
}

async fn session() {
    let (tx, mut rx) = mpsc::channel(32);

    let reader = tokio::spawn(async move {
        let mut stdin = io::BufReader::new(io::stdin()).lines();
        println!("Type stuff:");
        while let Some(line) = stdin.next_line().await.unwrap() {
            tx.send(line).await.unwrap();
            println!("Type stuff:");
        }
        println!("Done reading!");
    });

    let writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            println!("Got line: {}", line);
        }
        println!("Done writing!");
    });

    reader.await.unwrap();
    writer.await.unwrap();
    println!("Done with everything!");
    ()
}
