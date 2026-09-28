use super::*;

fn assert_initial_worker_auth_network_headers(headers: Option<&[(String, String)]>) {
    let headers = headers.expect("worker auth transport request headers");
    assert!(
        headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("host") && !value.is_empty()),
        "worker auth transport headers should contain Host: {headers:?}"
    );
    assert!(
        headers
            .iter()
            .all(|(name, _)| !name.eq_ignore_ascii_case("authorization")),
        "the browser-visible auth request observation must remain the initial unauthenticated exchange: {headers:?}"
    );
}

mod fetch_network;
mod file_and_xhr;
mod opfs_storage;
mod websockets;
mod worker_globals;
mod worker_storage_security;
