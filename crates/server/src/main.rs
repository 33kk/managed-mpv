use std::error::Error;
use std::os::fd::AsRawFd;

use actix_web::http::StatusCode;
use actix_web::{get, web, App, HttpResponse, HttpResponseBuilder, HttpServer};
use path_macro::path;

mod mpv;
use mpv::{get_id, Mpv};
use serde::Deserialize;
use tokio::fs;
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

struct State {
	process: Child,
	client: Mpv,
}

type SharedState = web::Data<Mutex<Option<State>>>;

async fn notify(title: &str, description: &str) -> Result<(), Box<dyn Error>> {
	Command::new("notify-send")
		.args(["--app-name=mpv", title, description])
		.spawn()?
		.wait()
		.await?;

	Ok(())
}

async fn mpv_ensure_running(state: SharedState) -> Result<(), Box<dyn Error>> {
	if let Some(state) = &mut *state.lock().await {
		if state.process.try_wait()?.is_none() {
			return Ok(());
		}
	}
	let (rust_socket, mpv_socket) = UnixStream::pair()?;

	let fd = rust_socket.as_raw_fd();

	// Unset FD_CLOEXEC on the socket to be passed to the child.
	unsafe {
		let flags = libc::fcntl(fd, libc::F_GETFD);
		libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
	}

	let client = Mpv::new(mpv_socket);

	let process = Command::new("mpv")
		.args([
			"--player-operation-mode=pseudo-gui",
			format!("--input-ipc-client=fd://{fd}").as_str(),
		])
		.spawn()?;

	{
		let mut client = client.clone();
		tokio::spawn(async move { client.listen().await });
	}

	*state.lock().await = Some(State {
		process,
		client,
	});

	Ok(())
}

#[derive(Deserialize)]
struct PlayQuery {
	pub url: String,
	pub title: String,
}

#[get("/play")]
async fn play(
	query: web::Query<PlayQuery>,
	state: SharedState,
) -> Result<HttpResponse, Box<dyn Error>> {
	{
		let state = state.clone();
		mpv_ensure_running(state).await?;
	}

	if let Some(state) = &mut *state.lock().await {
		let list_path = path!(
			std::env::var("XDG_RUNTIME_DIR").unwrap()
				/ format!("managed-mpv-list-{}.m3u", get_id())
		);

		let title = if query.title.is_empty() {
			query.url.clone()
		} else {
			format!("{} ({})", query.title, query.url)
		};

		state
			.client
			.command(&["show-text", format!("Adding to playlist: {title}").as_str()])
			.await?;

		drop(notify("Adding to playlist", title.as_str()).await);

		fs::write(
			&list_path,
			format!("#EXTM3U\n#EXTINF:-1,{title}\n{}", query.url),
		)
		.await?;

		let response = state
			.client
			.command(&["loadlist", &list_path.to_string_lossy(), "append-play"])
			.await;

		fs::remove_file(list_path).await?;

		Ok(HttpResponseBuilder::new(StatusCode::OK).json(response?))
	} else {
		Ok(HttpResponseBuilder::new(StatusCode::INTERNAL_SERVER_ERROR)
			.body("Error: Failed to start mpv."))
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	tracing_subscriber::fmt::init();

	let state: SharedState = web::Data::new(Mutex::new(None));

	let path = path!(&std::env::var("XDG_RUNTIME_DIR")? / "managed-mpv");
	drop(fs::remove_file(&path).await);

	HttpServer::new(move || App::new().app_data(web::Data::clone(&state)).service(play))
		.bind_uds(path)?
		.run()
		.await?;

	Ok(())
}
