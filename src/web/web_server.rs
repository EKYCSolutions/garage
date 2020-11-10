use std::net::SocketAddr;
use std::sync::Arc;

use futures::future::Future;

use hyper::header::HOST;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};

use garage_model::garage::Garage;
use garage_util::error::Error;

pub async fn run_web_server(
	garage: Arc<Garage>,
	shutdown_signal: impl Future<Output = ()>,
) -> Result<(), Error> {
	let addr = &garage.config.s3_web.bind_addr;

	let service = make_service_fn(|conn: &AddrStream| {
		let garage = garage.clone();
		let client_addr = conn.remote_addr();
		async move {
			Ok::<_, Error>(service_fn(move |req: Request<Body>| {
				let garage = garage.clone();
				handler(garage, req, client_addr)
			}))
		}
	});

	let server = Server::bind(&addr).serve(service);
	let graceful = server.with_graceful_shutdown(shutdown_signal);
	info!("Web server listening on http://{}", addr);

	graceful.await?;
	Ok(())
}

async fn handler(
	garage: Arc<Garage>,
	req: Request<Body>,
	addr: SocketAddr,
) -> Result<Response<Body>, Error> {
	// Get http authority string (eg. [::1]:3902 or garage.tld:80)
	let authority = req
		.headers()
		.get(HOST)
		.ok_or(Error::BadRequest(format!("HOST header required")))?
		.to_str()?;

	// Get bucket
	let host = authority_to_host(authority)?;
	let root = &garage.config.s3_web.root_domain;
	let bucket = host_to_bucket(&host, root);

	// Get path
	let path = req.uri().path().to_string();
	let key = percent_encoding::percent_decode_str(&path).decode_utf8()?;

	info!("Selected bucket: \"{}\", selected key: \"{}\"", bucket, key);

	Ok(Response::new(Body::from("hello world\n")))
}

/// Extract host from the authority section given by the HTTP host header
///
/// The HTTP host contains both a host and a port.
/// Extracting the port is more complex than just finding the colon (:) symbol due to IPv6
/// we do not use the collect pattern as there is no way in std rust to collect over a stack allocated value
/// check here: https://docs.rs/collect_slice/1.2.0/collect_slice/
fn authority_to_host(authority: &str) -> Result<&str, Error> {
	let mut iter = authority.chars().enumerate();
	let split = match iter.next() {
    Some((_, '[')) => {
			let mut niter = iter.skip_while(|(_, c)| c != &']');
			niter.next().ok_or(Error::BadRequest(format!("Authority {} has an illegal format", authority)))?;
			niter.next()
		}, 
		Some((_, _)) => iter.skip_while(|(_, c)| c != &':').next(),
		None => return Err(Error::BadRequest(format!("Authority is empty"))),
	};

  match split {
		Some((i, ':')) => Ok(&authority[..i]),
		None => Ok(authority),
		Some((_, _)) => Err(Error::BadRequest(format!("Authority {} has an illegal format", authority))),
	}
}


fn host_to_bucket<'a>(host: &'a str, root: &str) -> &'a str {
	if root.len() >= host.len() || !host.ends_with(root) {
		return host;
	}

	let len_diff = host.len() - root.len();
	let missing_starting_dot = root.chars().next() != Some('.');
	let cursor = if missing_starting_dot {
		len_diff - 1
	} else {
		len_diff
	};
	&host[..cursor]
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn authority_to_host_with_port() -> Result<(), Error> {
		let domain = authority_to_host("[::1]:3902")?;
		assert_eq!(domain, "[::1]");
		let domain2 = authority_to_host("garage.tld:65200")?;
		assert_eq!(domain2, "garage.tld");
		let domain3 = authority_to_host("127.0.0.1:80")?;
		assert_eq!(domain3, "127.0.0.1");
		Ok(())
	}

	#[test]
	fn authority_to_host_without_port() -> Result<(), Error> {
		let domain = authority_to_host("[::1]")?;
		assert_eq!(domain, "[::1]");
		let domain2 = authority_to_host("garage.tld")?;
		assert_eq!(domain2, "garage.tld");
		let domain3 = authority_to_host("127.0.0.1")?;
		assert_eq!(domain3, "127.0.0.1");
		Ok(())
	}

	#[test]
	fn host_to_bucket_test() {
		assert_eq!(
			host_to_bucket("john.doe.garage.tld", ".garage.tld"),
			"john.doe"
		);

		assert_eq!(
			host_to_bucket("john.doe.garage.tld", "garage.tld"),
			"john.doe"
		);

		assert_eq!(host_to_bucket("john.doe.com", "garage.tld"), "john.doe.com");

		assert_eq!(
			host_to_bucket("john.doe.com", ".garage.tld"),
			"john.doe.com"
		);

		assert_eq!(host_to_bucket("garage.tld", "garage.tld"), "garage.tld");

		assert_eq!(host_to_bucket("garage.tld", ".garage.tld"), "garage.tld");
	}
}
