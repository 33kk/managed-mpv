use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::SinkExt;
use serde_json::json;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::{oneshot, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};

pub fn get_id() -> u64 {
	static COUNTER: AtomicU64 = AtomicU64::new(1);
	COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("{0}")]
	Io(#[from] tokio::io::Error),
	#[error("{0}")]
	Serialization(#[from] serde_json::Error),
	#[error("{0}")]
	LinesCodec(#[from] LinesCodecError),
	#[error("{0}")]
	RecvError(#[from] RecvError),
}

#[derive(Clone)]
pub struct Mpv {
	read: Arc<Mutex<FramedRead<OwnedReadHalf, LinesCodec>>>,
	write: Arc<Mutex<FramedWrite<OwnedWriteHalf, LinesCodec>>>,
	requests: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
}

impl Mpv {
	pub fn new(socket: UnixStream) -> Mpv {
		let (read, write) = socket.into_split();
		let read = FramedRead::new(read, LinesCodec::new());
		let write = FramedWrite::new(write, LinesCodec::new());

		Mpv {
			read: Arc::new(Mutex::new(read)),
			write: Arc::new(Mutex::new(write)),
			requests: Arc::new(Mutex::new(HashMap::new())),
		}
	}

	pub async fn listen(&mut self) -> Result<(), Error> {
		while let Some(line) = self.read.lock().await.try_next().await? {
			let json: serde_json::Value = serde_json::from_str(line.as_str())?;
			if let Some(data) = json.as_object() {
				if let Some(Some(id)) = data.get("request_id").map(serde_json::Value::as_u64) {
					if let Some(tx) = self.requests.lock().await.remove(&id) {
						drop(tx.send(json));
					}
				}
			}
		}

		Ok(())
	}

	pub async fn command(&mut self, args: &[&str]) -> Result<serde_json::Value, Error> {
		let id = get_id();
		let (tx, rx) = oneshot::channel();

		self.requests.lock().await.insert(id, tx);

		let request = json!({ "request_id": id, "command": args });
		self.write.lock().await.send(request.to_string()).await?;

		let response = rx.await?;

		Ok(response)
	}
}
