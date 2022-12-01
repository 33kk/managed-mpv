use std::env;
use std::error::Error;
use std::str::FromStr;

use hyper::body::Bytes;
use hyper::client::connect::Connect;
use hyper::{Client, Uri};
use hyper_tls::HttpsConnector;
use hyperlocal::UnixClientExt;
use regex::Regex;

type BoxError = Box<dyn Error + Send + Sync>;

async fn get<C: Connect + Clone + Send + Sync + 'static>(
	client: &Client<C>,
	url: Uri,
) -> Result<(hyper::http::response::Parts, Bytes), BoxError> {
	let response = client.get(url).await?;
	let (parts, body) = response.into_parts();
	let bytes = hyper::body::to_bytes(body).await?;

	Ok((parts, bytes))
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
	let args: Vec<String> = env::args().skip(1).collect();

	let https = HttpsConnector::new();
	let client = Client::builder().build::<_, hyper::Body>(https);

	println!("{}", &args[0]);
	let video_url = hyper::Uri::from_str(&args[0])?;

	let video_title = match video_url.host().unwrap() {
		"www.youtube.com" => {
			let (_parts, body) = get(&client, video_url.clone()).await?;
			let body = std::str::from_utf8(&*body).unwrap();

			let re = Regex::new("<title>(.*?)(?: - YouTube)?</title>")?;
			let title = re.captures(body).unwrap().get(1).unwrap().as_str();
			quick_xml::escape::unescape(title)?.to_string()
		}
		_ => video_url.to_string(),
	};

	let video_url = urlencoding::encode(&args[0]);
	let video_title = urlencoding::encode(&video_title);

	println!("{}", video_title);

	let client = Client::unix();
	let socket_url: hyper::Uri = hyperlocal::Uri::new(
		format!("{}/managed-mpv", env::var("XDG_RUNTIME_DIR").unwrap()).as_str(),
		format!("/play?url={video_url}&title={video_title}").as_str(),
	)
	.into();

	let (_parts, bytes) = get(&client, socket_url).await?;

	println!("{:#?}", bytes);

	Ok(())
}
