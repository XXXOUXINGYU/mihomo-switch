#[path = "../src/logging.rs"]
mod logging;
#[path = "../src/models.rs"]
mod models;
#[path = "../src/parser.rs"]
mod parser;
#[path = "../src/runtime_paths.rs"]
mod runtime_paths;

use anyhow::Context;
use reqwest::{Client, Proxy};
use std::time::Instant;

fn build_client(verify_tls: bool, proxy_url: Option<&str>) -> anyhow::Result<Client> {
    let mut builder = Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(!verify_tls)
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(8));
    if let Some(proxy_url) = proxy_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.proxy(Proxy::all(proxy_url).context("invalid proxy url")?);
    }
    builder.build().map_err(Into::into)
}

fn main() {
    let url = std::env::var("FETCH_DEBUG_URL")
        .unwrap_or_else(|_| "https://example.invalid/subscription".to_string());
    let cases = [
        ("direct", false, ""),
        ("proxy", true, "http://127.0.0.1:20122"),
    ];

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    for (label, local_proxy_enabled, local_proxy_url) in cases {
        for verify_tls in [true, false] {
            let started = Instant::now();
            let result = runtime.block_on(async {
                let client = build_client(
                    verify_tls,
                    local_proxy_enabled.then_some(local_proxy_url),
                )?;
                let response = client
                    .get(&url)
                    .header("User-Agent", "clash-verge/2.2.3")
                    .send()
                    .await?;
                println!(
                    "{label} verify_tls={verify_tls}: status={} version={:?} headers_ct={:?} in {:?}",
                    response.status(),
                    response.version(),
                    response.headers().get("content-type"),
                    started.elapsed()
                );
                let text = response.text().await?;
                println!(
                    "{label} verify_tls={verify_tls}: text_len={} starts_with_mixed={} at {:?}",
                    text.len(),
                    text.starts_with("mixed-port:"),
                    started.elapsed()
                );
                Ok::<(), anyhow::Error>(())
            });
            match result {
                Ok(()) => {}
                Err(error) => {
                    println!(
                        "{label} verify_tls={verify_tls}: manual reqwest error in {:?}: {error}",
                        started.elapsed()
                    );
                }
            }
        }

        let started = Instant::now();
        let result = runtime.block_on(parser::fetch_subscription_headless(
            &url,
            Some("clash-verge/2.2.3"),
            "",
            local_proxy_enabled,
            local_proxy_url,
        ));
        match result {
            Ok(text) => {
                let parsed = parser::detect_and_parse(&text).expect("parse fetched text");
                println!(
                    "{label}: ok in {:?}, bytes={}, nodes={}",
                    started.elapsed(),
                    text.len(),
                    parsed.len()
                );
            }
            Err(error) => {
                println!("{label}: error in {:?}: {error}", started.elapsed());
            }
        }
    }
}
