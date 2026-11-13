use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let result = runtime.block_on(async {
        let (mut reader, mut writer) = tokio::io::duplex(16);
        let task = tokio::spawn(async move {
            writer.write_all(b"async IO").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        });
        let mut text = String::new();
        reader.read_to_string(&mut text).await.unwrap();
        task.await.unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        sender.send(text).await.unwrap();
        receiver.recv().await.unwrap()
    });
    assert_eq!(result, "async IO");
    println!("{}", result);
}
